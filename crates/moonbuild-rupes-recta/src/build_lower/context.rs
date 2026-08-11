// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Lowering context and core implementation.

use std::path::{Path, PathBuf};

use moonutil::{
    resolution::{DirSyncResult, ResolvedEnv},
    target::TargetBackend,
};
use tracing::{Level, instrument};
use walkdir::WalkDir;

use super::{
    BuildOptions, CExecutableRealization, CStubLibraryRealization, LoweredAction, LoweredArtifact,
    LoweredExternalInput, LoweringError,
};
use crate::{
    ResolveOutput,
    build_action_plan::{BuildAction, BuildActionId, BuildActionPlan},
    build_plan::ArtifactKey,
    discover::{DiscoverResult, DiscoveredPackage},
    model::{BackendConfig, BuildTarget},
    pkg_solve::DepRelationship,
    target_layout::ArtifactPathResolver,
};

pub(crate) struct LoweringContext<'a> {
    // Physical paths for logical build artifacts.
    pub(crate) artifact_paths: ArtifactPathResolver,

    // External state
    pub(crate) packages: &'a DiscoverResult,
    pub(crate) modules: &'a ResolvedEnv,
    pub(crate) module_dirs: &'a DirSyncResult,
    pub(crate) rel: &'a DepRelationship,
    pub(crate) plan: &'a BuildActionPlan<'a>,
    pub(crate) opt: &'a BuildOptions,

    // Native compilation observes the selected Moon toolchain include tree.
    // Discover it at most once for all actions lowered by this context.
    toolchain_include_files: Option<Vec<PathBuf>>,
}

pub(super) struct ActionArtifacts {
    outputs: Vec<LoweredArtifact>,
    dependencies: Vec<LoweredArtifact>,
}

impl ActionArtifacts {
    fn new(ctx: &LoweringContext<'_>, action: BuildActionId) -> Self {
        let outputs = ctx
            .plan
            .output_artifacts(action)
            .into_iter()
            .map(|artifact| Self::realize(ctx, action, artifact))
            .collect();
        let dependencies = ctx
            .plan
            .dependency_artifacts(action)
            .into_iter()
            .map(|(dependency_action, artifact)| Self::realize(ctx, dependency_action, artifact))
            .collect();
        Self {
            outputs,
            dependencies,
        }
    }

    fn realize(
        ctx: &LoweringContext<'_>,
        provider_action: BuildActionId,
        artifact: ArtifactKey,
    ) -> LoweredArtifact {
        let paths = ctx.artifact_paths.paths_for_artifact(
            &artifact,
            ctx.plan.action(provider_action),
            ctx.packages,
            ctx.modules,
            ctx.opt.artifact_path_options(),
        );
        LoweredArtifact {
            producer: provider_action,
            artifact,
            paths,
        }
    }

    pub(super) fn single_output_path(&self) -> PathBuf {
        match self.outputs.as_slice() {
            [artifact] => Self::optional_single_realized_path(artifact)
                .unwrap_or_else(|| unreachable!("expected exactly one path for artifact")),
            [] => unreachable!("expected exactly one output artifact"),
            _ => unreachable!(
                "expected one output artifact, got {:?}",
                self.outputs
                    .iter()
                    .map(|realized| &realized.artifact)
                    .collect::<Vec<_>>()
            ),
        }
    }

    pub(super) fn single_output_path_matching(
        &self,
        matches: impl Fn(&ArtifactKey) -> bool,
    ) -> PathBuf {
        self.optional_single_output_path_matching(matches)
            .unwrap_or_else(|| unreachable!("expected one matching output artifact"))
    }

    pub(super) fn optional_single_output_path_matching(
        &self,
        matches: impl Fn(&ArtifactKey) -> bool,
    ) -> Option<PathBuf> {
        Self::single_matching_path(&self.outputs, matches)
    }

    pub(super) fn single_dependency_path_matching(
        &self,
        matches: impl Fn(&ArtifactKey) -> bool,
    ) -> PathBuf {
        Self::single_matching_path(&self.dependencies, matches)
            .unwrap_or_else(|| unreachable!("expected one matching dependency artifact"))
    }

    pub(super) fn dependency_paths_matching(
        &self,
        matches: impl Fn(&ArtifactKey) -> bool,
    ) -> Vec<PathBuf> {
        self.dependencies
            .iter()
            .filter(|realized| matches(&realized.artifact))
            .flat_map(|realized| realized.paths.iter().cloned())
            .collect()
    }

    fn single_matching_path(
        realized: &[LoweredArtifact],
        matches: impl Fn(&ArtifactKey) -> bool,
    ) -> Option<PathBuf> {
        let matched = realized
            .iter()
            .filter(|realized| matches(&realized.artifact))
            .collect::<Vec<_>>();
        match matched.as_slice() {
            [artifact] => Self::optional_single_realized_path(artifact),
            [] => None,
            _ => unreachable!("expected at most one matching artifact"),
        }
    }

    fn optional_single_realized_path(artifact: &LoweredArtifact) -> Option<PathBuf> {
        match artifact.paths.as_slice() {
            [path] => Some(path.clone()),
            [] => None,
            _ => unreachable!(
                "expected one path for artifact, got {:?}: {:?}",
                artifact.paths, artifact.artifact
            ),
        }
    }
}

impl<'a> LoweringContext<'a> {
    pub(super) fn new(
        artifact_paths: ArtifactPathResolver,
        resolve_output: &'a ResolveOutput,
        plan: &'a BuildActionPlan<'a>,
        opt: &'a BuildOptions,
    ) -> Self {
        Self {
            artifact_paths,
            rel: &resolve_output.pkg_rel,
            modules: &resolve_output.module_rel,
            packages: &resolve_output.pkg_dirs,
            module_dirs: &resolve_output.module_dirs,
            plan,
            opt,
            toolchain_include_files: None,
        }
    }

    fn toolchain_include_files(&mut self) -> Result<&[PathBuf], LoweringError> {
        if self.toolchain_include_files.is_none() {
            let root = PathBuf::from(&self.opt.compiler_paths().include_path);
            let mut files = Vec::new();
            for entry in WalkDir::new(&root).follow_links(true).sort_by_file_name() {
                let entry = entry.map_err(|source| LoweringError::ToolchainInclude {
                    path: root.clone(),
                    source,
                })?;
                if entry.file_type().is_file() {
                    files.push(entry.into_path());
                }
            }
            files.sort();
            self.toolchain_include_files = Some(files);
        }
        Ok(self
            .toolchain_include_files
            .as_deref()
            .expect("toolchain include files should be initialized"))
    }

    /// Some actions are no-op in n2 build graph. Early bailing.
    fn is_action_noop(&self, action: BuildAction<'_>) -> bool {
        (!self.opt.target_backend().is_native())
            && matches!(action, BuildAction::MakeExecutable { .. })
    }

    pub(super) fn get_package(&self, target: BuildTarget) -> &DiscoveredPackage {
        self.packages.get_package(target.package)
    }

    pub(super) fn output_paths_for_action(&self, action: BuildActionId) -> Vec<PathBuf> {
        let mut paths = self
            .plan
            .output_artifacts(action)
            .into_iter()
            .flat_map(|artifact| {
                self.artifact_paths.paths_for_artifact(
                    &artifact,
                    self.plan.action(action),
                    self.packages,
                    self.modules,
                    self.opt.artifact_path_options(),
                )
            })
            .collect::<Vec<_>>();
        if let BuildAction::MakeExecutable { target, .. } = self.plan.action(action)
            && self.plan.generates_dsym_for_target(&target)
        {
            paths.push(
                self.artifact_paths
                    .target_layout()
                    .dsym_bundle_of_build_target(
                        self.packages,
                        &target,
                        self.opt.artifact_path_options().executable,
                    ),
            );
        }
        paths
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn lower_action(
        &mut self,
        id: BuildActionId,
    ) -> Result<Option<LoweredAction>, LoweringError> {
        let action = self.plan.action(id);
        if self.is_action_noop(action) {
            return Ok(None);
        }
        let action_artifacts = ActionArtifacts::new(self, id);

        // Lower the action to its command and tool-specific execution transport.
        let cmd = match action {
            BuildAction::Check { target, info } => {
                self.lower_check(&action_artifacts, target, info)?
            }
            BuildAction::EmitProof { target, info } => {
                self.lower_emit_proof(&action_artifacts, target, info)?
            }
            BuildAction::Prove { target, info } => {
                self.lower_prove(&action_artifacts, target, info)?
            }
            BuildAction::BuildCore { target, info } => {
                self.lower_build_mbt(&action_artifacts, target, info)?
            }
            BuildAction::BuildCStub {
                package,
                index,
                info,
            } => self.lower_build_c_stub(&action_artifacts, package, index, info),
            BuildAction::ArchiveOrLinkCStubs { package, info } => {
                self.lower_archive_or_link_c_stubs(&action_artifacts, package, info)
            }
            BuildAction::LinkCore {
                target,
                info,
                make_executable_info,
            } => self.lower_link_core(&action_artifacts, target, info, make_executable_info)?,
            BuildAction::MakeExecutable {
                target,
                info: Some(info),
            } => self.lower_make_exe(&action_artifacts, target, info),
            BuildAction::MakeExecutable { info: None, .. } => {
                panic!("native MakeExecutable actions should have executable info")
            }
            BuildAction::GenerateDsym { target, dsymutil } => {
                self.lower_generate_dsym(&action_artifacts, target, dsymutil)
            }
            BuildAction::GenerateTestInfo { target, info } => {
                self.lower_gen_test_driver(&action_artifacts, target, info)
            }
            BuildAction::GenerateMbti { target } => {
                self.lower_generate_mbti(&action_artifacts, target)
            }
            BuildAction::BuildVirtual { package, input } => {
                self.lower_parse_mbti(package, input)?
            }
            BuildAction::Bundle { module, targets } => {
                self.lower_bundle(&action_artifacts, module, targets)?
            }
            BuildAction::BuildRuntimeObject { index, info } => {
                self.lower_compile_runtime_object(&action_artifacts, index, info)
            }
            BuildAction::BuildRuntimeLib { info } => {
                self.lower_build_runtime_lib(&action_artifacts, info)
            }
            BuildAction::BuildDocs { module } => self.lower_build_docs(module),
            BuildAction::RunPrebuild { info, .. } => self.lower_run_prebuild(info)?,
            BuildAction::RunMoonLexPrebuild { input, output, .. } => {
                self.lower_moon_lex_prebuild(input, output)
            }
            BuildAction::RunMoonYaccPrebuild { input, output, .. } => {
                self.lower_moon_yacc_prebuild(input, output)
            }
        };

        let (command, mut external_inputs) = cmd.into_lowered_parts(&action_artifacts.dependencies);
        if matches!(
            action,
            BuildAction::Check { .. }
                | BuildAction::EmitProof { .. }
                | BuildAction::Prove { .. }
                | BuildAction::BuildCore { .. }
                | BuildAction::BuildVirtual { .. }
        ) && let Some(stdlib_root) = &self.opt.stdlib_path
        {
            external_inputs.push(LoweredExternalInput::StandardLibraryInterfaces(
                moonutil::toolchain::core_bundle_in(stdlib_root, self.opt.target_backend()),
            ));
        }

        let observes_toolchain_headers = matches!(
            action,
            BuildAction::BuildCStub { .. } | BuildAction::BuildRuntimeObject { .. }
        ) || matches!(
            action,
            BuildAction::BuildRuntimeLib { .. } if self.opt.backend.uses_shared_runtime()
        ) || matches!(
            action,
            BuildAction::MakeExecutable { .. }
                if matches!(
                    &self.opt.backend,
                    BackendConfig::Native(backend)
                        if backend.executable_realization()
                            == CExecutableRealization::CompileAndLinkGeneratedC
                )
        );
        if observes_toolchain_headers {
            external_inputs.extend(
                self.toolchain_include_files()?
                    .iter()
                    .cloned()
                    .map(LoweredExternalInput::File),
            );
        }

        // These are the only Moon-owned libraries that command construction
        // may append as standalone argv. Compare their exact rendered paths;
        // arbitrary command arguments remain opaque to lowering.
        for name in ["libmoonbitrun.o", "libbacktrace.a"] {
            let path = Path::new(&self.opt.compiler_paths().lib_path).join(name);
            let rendered = path.display().to_string();
            if command.args().iter().any(|argument| argument == &rendered) {
                external_inputs.push(LoweredExternalInput::File(path));
            }
        }
        external_inputs.sort();
        external_inputs.dedup();

        let error_package = self
            .plan
            .package_for_error(id)
            .map(|target| self.get_package(target).fqn.clone())
            .into();
        // Keep this exhaustive: adding an action must require an explicit
        // decision that lowering describes every filesystem observation.
        let cache_eligible = match action {
            BuildAction::Check { target, .. }
            | BuildAction::EmitProof { target, .. }
            | BuildAction::BuildCore { target, .. } => {
                let package = self.get_package(target);
                self.packages
                    .module_info(package.module)
                    .compile_flags
                    .as_deref()
                    .is_none_or(<[_]>::is_empty)
            }
            BuildAction::BuildCStub { info, .. } => info.cc_flags.is_empty(),
            BuildAction::ArchiveOrLinkCStubs { info, .. } => {
                self.opt.backend.c_stub_library_realization()
                    == CStubLibraryRealization::StaticArchive
                    || info.link_flags.is_empty()
            }
            BuildAction::LinkCore { target, .. } => {
                let package = self.get_package(target);
                let package_link_flags =
                    package
                        .raw
                        .link
                        .as_ref()
                        .and_then(|link| match self.opt.target_backend() {
                            TargetBackend::Wasm => link
                                .wasm
                                .as_ref()
                                .and_then(|config| config.flags.as_deref()),
                            TargetBackend::WasmGC => link
                                .wasm_gc
                                .as_ref()
                                .and_then(|config| config.flags.as_deref()),
                            TargetBackend::Js | TargetBackend::Native | TargetBackend::LLVM => None,
                        });
                self.packages
                    .module_info(package.module)
                    .link_flags
                    .as_deref()
                    .is_none_or(<[_]>::is_empty)
                    && package_link_flags.is_none_or(<[_]>::is_empty)
            }
            BuildAction::MakeExecutable {
                info: Some(info), ..
            } => match &self.opt.backend {
                BackendConfig::Native(backend) => match backend.executable_realization() {
                    CExecutableRealization::CompileAndLinkGeneratedC => {
                        info.c_flags.is_empty() && info.link_flags.is_empty()
                    }
                    CExecutableRealization::LinkDirectObject => info.link_flags.is_empty(),
                    CExecutableRealization::WriteTccRunResponseFile => true,
                },
                BackendConfig::Llvm => info.c_flags.is_empty() && info.link_flags.is_empty(),
                BackendConfig::Wasm { .. } | BackendConfig::WasmGc { .. } | BackendConfig::Js => {
                    unreachable!("non-native make-executable actions are no-ops")
                }
            },
            BuildAction::MakeExecutable { info: None, .. } => {
                unreachable!("native MakeExecutable actions should have executable info")
            }
            BuildAction::GenerateDsym { .. }
            | BuildAction::GenerateTestInfo { .. }
            | BuildAction::GenerateMbti { .. }
            | BuildAction::BuildVirtual { .. }
            | BuildAction::Bundle { .. }
            | BuildAction::BuildRuntimeObject { .. }
            | BuildAction::BuildRuntimeLib { .. }
            | BuildAction::RunMoonLexPrebuild { .. }
            | BuildAction::RunMoonYaccPrebuild { .. } => true,
            // These actions still observe filesystem state that is broader
            // than the concrete files represented by the lowered action.
            BuildAction::Prove { .. }
            | BuildAction::BuildDocs { .. }
            | BuildAction::RunPrebuild { .. } => false,
        };
        Ok(Some(LoweredAction {
            id,
            dependencies: action_artifacts.dependencies,
            external_inputs,
            outputs: action_artifacts.outputs,
            command,
            cache_eligible,
            fileloc: self.plan.fileloc(id, self.modules, self.packages),
            description: self.plan.human_desc(id, self.modules, self.packages),
            can_dirty_on_output: self.plan.can_dirty_on_output(id),
            error_package,
        }))
    }
}
