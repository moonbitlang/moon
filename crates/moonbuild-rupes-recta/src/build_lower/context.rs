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

use super::{BuildOptions, CExecutableRealization, CStubLibraryRealization, LoweringError};
use crate::{
    ResolveOutput,
    build_plan::{
        ArtifactKey, BuildAction, BuildPlan, BuildPlanActionKey, PackagePrebuildAction,
        PackagePrebuildKey, package_file_key,
    },
    discover::{DiscoverResult, DiscoveredPackage},
    execution_plan::{ActionId, ExecutionAction, ExecutionPlanBuilder, InputObservation},
    model::{BackendConfig, BuildPlanNode, BuildTarget},
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
    pub(crate) plan: &'a BuildPlan,
    pub(crate) opt: &'a BuildOptions,

    // Native compilation observes the selected Moon toolchain include tree.
    // Discover it at most once for all actions lowered by this context.
    toolchain_include_files: Option<Vec<PathBuf>>,
}

pub(super) struct ActionArtifacts {
    outputs: Vec<RealizedArtifact>,
    dependencies: Vec<RealizedArtifact>,
}

pub(super) struct RealizedArtifact {
    artifact: ArtifactKey,
    paths: Vec<PathBuf>,
}

impl ActionArtifacts {
    fn new(ctx: &LoweringContext<'_>, action: &BuildPlanActionKey) -> Self {
        let outputs = ctx
            .plan
            .provided_artifacts(action)
            .map(|artifact| Self::realize(ctx, action, artifact))
            .collect();
        let dependencies = ctx
            .plan
            .artifact_dependencies(action)
            .map(|(dependency_action, artifact)| Self::realize(ctx, &dependency_action, artifact))
            .collect();
        Self {
            outputs,
            dependencies,
        }
    }

    fn realize(
        ctx: &LoweringContext<'_>,
        provider_action: &BuildPlanActionKey,
        artifact: ArtifactKey,
    ) -> RealizedArtifact {
        let paths = ctx.artifact_paths.paths_for_artifact(
            &artifact,
            ctx.action(provider_action),
            ctx.packages,
            ctx.modules,
            ctx.opt.artifact_path_options(),
        );
        RealizedArtifact { artifact, paths }
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
        realized: &[RealizedArtifact],
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

    fn optional_single_realized_path(artifact: &RealizedArtifact) -> Option<PathBuf> {
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
        plan: &'a BuildPlan,
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

    pub(super) fn get_package(&self, target: BuildTarget) -> &DiscoveredPackage {
        self.packages.get_package(target.package)
    }

    fn action(&self, action: &BuildPlanActionKey) -> BuildAction<'a> {
        let node = match action {
            BuildPlanActionKey::Backend(node) => *node,
            BuildPlanActionKey::PackagePrebuild(key) => {
                let action = self
                    .plan
                    .package_prebuild_action(key)
                    .expect("package prebuild key should name a planned action");
                return match (key, action) {
                    (PackagePrebuildKey::Custom { .. }, PackagePrebuildAction::Custom { info }) => {
                        BuildAction::RunPrebuild { info }
                    }
                    (
                        PackagePrebuildKey::MoonLex {
                            package,
                            input: key_input,
                        },
                        PackagePrebuildAction::MoonLex { input, output },
                    ) if key_input == input => BuildAction::RunMoonLexPrebuild {
                        package: *package,
                        input,
                        output,
                    },
                    (
                        PackagePrebuildKey::MoonYacc {
                            package,
                            input: key_input,
                        },
                        PackagePrebuildAction::MoonYacc { input, output },
                    ) if key_input == input => BuildAction::RunMoonYaccPrebuild {
                        package: *package,
                        input,
                        output,
                    },
                    _ => unreachable!("package prebuild key and action kind must agree"),
                };
            }
        };
        match node {
            BuildPlanNode::Check(target) => BuildAction::Check {
                target,
                info: self
                    .plan
                    .get_build_target_info(&target)
                    .expect("Build target info should be present for Check nodes"),
            },
            BuildPlanNode::EmitProof(target) => BuildAction::EmitProof {
                target,
                info: self
                    .plan
                    .get_build_target_info(&target)
                    .expect("Build target info should be present for EmitProof nodes"),
            },
            BuildPlanNode::Prove(target) => BuildAction::Prove {
                target,
                info: self
                    .plan
                    .get_build_target_info(&target)
                    .expect("Build target info should be present for Prove nodes"),
            },
            BuildPlanNode::BuildCore(target) => BuildAction::BuildCore {
                target,
                info: self
                    .plan
                    .get_build_target_info(&target)
                    .expect("Build target info should be present for BuildCore nodes"),
            },
            BuildPlanNode::BuildCStub(package, index) => BuildAction::BuildCStub {
                package,
                index,
                info: self
                    .plan
                    .get_c_stubs_info(package)
                    .expect("C stub info should be present for BuildCStub nodes"),
            },
            BuildPlanNode::ArchiveOrLinkCStubs(package) => BuildAction::ArchiveOrLinkCStubs {
                package,
                info: self
                    .plan
                    .get_c_stubs_info(package)
                    .expect("C stubs info should be present for BuildCStubs nodes"),
            },
            BuildPlanNode::LinkCore(target) => BuildAction::LinkCore {
                target,
                info: self
                    .plan
                    .get_link_core_info(&target)
                    .expect("Link core info should be present for LinkCore nodes"),
                make_executable_info: self.plan.get_make_executable_info(&target),
            },
            BuildPlanNode::MakeExecutable(target) => BuildAction::MakeExecutable {
                target,
                info: self
                    .plan
                    .get_make_executable_info(&target)
                    .expect("MakeExecutable nodes should contain native linking info"),
            },
            BuildPlanNode::GenerateDsym(target) => BuildAction::GenerateDsym {
                target,
                dsymutil: self
                    .plan
                    .get_dsymutil()
                    .expect("dsymutil should be present for GenerateDsym nodes"),
            },
            BuildPlanNode::GenerateTestInfo(target) => BuildAction::GenerateTestInfo {
                target,
                info: self
                    .plan
                    .get_build_target_info(&target)
                    .expect("Build target info should be present for GenerateTestInfo nodes"),
            },
            BuildPlanNode::GenerateNodeTestPackageConfig(package) => {
                BuildAction::GenerateNodeTestPackageConfig { package }
            }
            BuildPlanNode::GenerateMbti(target) => BuildAction::GenerateMbti { target },
            BuildPlanNode::BuildVirtual(package) => BuildAction::BuildVirtual {
                package,
                input: self
                    .plan
                    .virtual_contract_input(package)
                    .expect("virtual contract input should be selected during build planning"),
            },
            BuildPlanNode::Bundle(module) => BuildAction::Bundle {
                module,
                targets: &self
                    .plan
                    .bundle_info(module)
                    .expect("Bundle info should be present when lowering bundle node")
                    .bundle_targets,
            },
            BuildPlanNode::BuildRuntimeObject(index) => BuildAction::BuildRuntimeObject {
                index,
                info: self
                    .plan
                    .get_runtime_info()
                    .expect("Runtime info should be present for runtime object nodes"),
            },
            BuildPlanNode::BuildRuntimeLib => BuildAction::BuildRuntimeLib {
                info: self
                    .plan
                    .get_runtime_info()
                    .expect("Runtime info should be present for BuildRuntimeLib nodes"),
            },
            BuildPlanNode::BuildDocs(module) => BuildAction::BuildDocs { module },
        }
    }

    fn human_desc(&self, action_key: &BuildPlanActionKey, action: BuildAction<'_>) -> String {
        let generator_desc = |tool: &str, package, input: &Path| {
            let input_name = input.file_name().map_or_else(
                || input.display().to_string(),
                |name| name.to_string_lossy().into(),
            );
            format!("run {tool} {} {input_name}", self.packages.fqn(package))
        };
        let description = match (action_key, action) {
            (BuildPlanActionKey::Backend(node), _) => {
                return node.human_desc(
                    self.modules,
                    self.packages,
                    self.opt.target_backend().to_flag(),
                );
            }
            (BuildPlanActionKey::PackagePrebuild(key), BuildAction::RunPrebuild { info }) => {
                let outputs = info
                    .resolved_outputs
                    .iter()
                    .map(|output| {
                        output.file_name().map_or_else(
                            || output.display().to_string(),
                            |name| name.to_string_lossy().into_owned(),
                        )
                    })
                    .collect::<Vec<_>>();
                let outputs = if outputs.is_empty() {
                    "(no outputs)".to_string()
                } else {
                    outputs.join(", ")
                };
                format!("run script {} {outputs}", self.packages.fqn(key.package()))
            }
            (
                BuildPlanActionKey::PackagePrebuild(_),
                BuildAction::RunMoonLexPrebuild { package, input, .. },
            ) => generator_desc("moonlex", package, input),
            (
                BuildPlanActionKey::PackagePrebuild(_),
                BuildAction::RunMoonYaccPrebuild { package, input, .. },
            ) => generator_desc("moonyacc", package, input),
            _ => unreachable!("Build Plan action kind must agree with its hydrated action"),
        };
        format!("{description} (prebuild)")
    }

    fn string_id(&self, action: &BuildPlanActionKey) -> String {
        match action {
            BuildPlanActionKey::Backend(node) => node.string_id(self.modules, self.packages),
            BuildPlanActionKey::PackagePrebuild(PackagePrebuildKey::Custom {
                package,
                declaration_index,
            }) => format!(
                "{}@RunPrebuild_{}",
                self.packages.fqn(*package),
                declaration_index
            ),
            BuildPlanActionKey::PackagePrebuild(PackagePrebuildKey::MoonLex { package, input }) => {
                let package = self.packages.get_package(*package);
                let input = package_file_key(&package.root_path, input);
                format!("{}@RunMoonLexPrebuild_{}", package.fqn, input.display())
            }
            BuildPlanActionKey::PackagePrebuild(PackagePrebuildKey::MoonYacc {
                package,
                input,
            }) => {
                let package = self.packages.get_package(*package);
                let input = package_file_key(&package.root_path, input);
                format!("{}@RunMoonYaccPrebuild_{}", package.fqn, input.display())
            }
        }
    }

    #[instrument(level = Level::DEBUG, skip(self, execution))]
    pub(super) fn lower_action(
        &mut self,
        action_key: &BuildPlanActionKey,
        execution: &mut ExecutionPlanBuilder,
    ) -> Result<ActionId, LoweringError> {
        let action = self.action(action_key);
        let action_artifacts = ActionArtifacts::new(self, action_key);

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
            BuildAction::MakeExecutable { target, info } => {
                self.lower_make_exe(&action_artifacts, target, info)
            }
            BuildAction::GenerateDsym { target, dsymutil } => {
                self.lower_generate_dsym(&action_artifacts, target, dsymutil)
            }
            BuildAction::GenerateTestInfo { target, info } => {
                self.lower_gen_test_driver(&action_artifacts, target, info)
            }
            BuildAction::GenerateNodeTestPackageConfig { package } => {
                self.lower_generate_node_test_package_config(&action_artifacts, package)
            }
            BuildAction::GenerateMbti { target } => {
                self.lower_generate_mbti(&action_artifacts, target)
            }
            BuildAction::BuildVirtual { package, input } => {
                self.lower_parse_mbti(&action_artifacts, package, input)?
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

        let (command, mut inputs) = cmd.into_lowered_parts(
            action_artifacts
                .dependencies
                .iter()
                .flat_map(|artifact| artifact.paths.iter().map(PathBuf::as_path)),
        );
        if matches!(
            action,
            BuildAction::Check { .. }
                | BuildAction::EmitProof { .. }
                | BuildAction::Prove { .. }
                | BuildAction::BuildCore { .. }
                | BuildAction::BuildVirtual { .. }
        ) && let Some(stdlib_root) = &self.opt.stdlib_path
        {
            inputs.push(InputObservation::StandardLibraryInterfaces(
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
                    BackendConfig::Native { mode, .. }
                        if mode.executable_realization()
                            == CExecutableRealization::CompileAndLinkGeneratedC
                )
        );
        if observes_toolchain_headers {
            inputs.extend(
                self.toolchain_include_files()?
                    .iter()
                    .cloned()
                    .map(InputObservation::File),
            );
        }

        // These are the only Moon-owned libraries that command construction
        // may append as standalone argv. Compare their exact rendered paths;
        // arbitrary command arguments remain opaque to lowering.
        for name in ["libmoonbitrun.o", "libbacktrace.a"] {
            let path = Path::new(&self.opt.compiler_paths().lib_path).join(name);
            let rendered = path.display().to_string();
            if command.args().iter().any(|argument| argument == &rendered) {
                inputs.push(InputObservation::File(path));
            }
        }
        inputs.extend(
            action_artifacts
                .dependencies
                .iter()
                .flat_map(|artifact| artifact.paths.iter().cloned())
                .map(InputObservation::File),
        );

        let error_package = match action_key {
            BuildPlanActionKey::Backend(node) => node.extract_target(),
            BuildPlanActionKey::PackagePrebuild(_) => None,
        }
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
            BuildAction::MakeExecutable { info, .. } => match &self.opt.backend {
                BackendConfig::Native { mode, .. } => match mode.executable_realization() {
                    CExecutableRealization::CompileAndLinkGeneratedC => {
                        info.c_flags.is_empty() && info.link_flags.is_empty()
                    }
                    CExecutableRealization::LinkDirectObject => info.link_flags.is_empty(),
                    CExecutableRealization::WriteTccRunResponseFile => true,
                },
                BackendConfig::Llvm { .. } => info.c_flags.is_empty() && info.link_flags.is_empty(),
                BackendConfig::Wasm { .. } | BackendConfig::WasmGc { .. } | BackendConfig::Js => {
                    unreachable!("non-native plans do not contain MakeExecutable actions")
                }
            },
            BuildAction::GenerateDsym { .. }
            | BuildAction::GenerateTestInfo { .. }
            | BuildAction::GenerateNodeTestPackageConfig { .. }
            | BuildAction::GenerateMbti { .. }
            | BuildAction::BuildVirtual { .. }
            | BuildAction::Bundle { .. }
            | BuildAction::BuildRuntimeObject { .. }
            | BuildAction::BuildRuntimeLib { .. }
            | BuildAction::RunMoonLexPrebuild { .. }
            | BuildAction::RunMoonYaccPrebuild { .. } => true,
            // These actions still observe filesystem state that is broader
            // than the concrete files represented by the execution action.
            BuildAction::Prove { .. }
            | BuildAction::BuildDocs { .. }
            | BuildAction::RunPrebuild { .. } => false,
        };
        let declared_outputs = match action {
            BuildAction::GenerateDsym { target, .. } => vec![
                self.artifact_paths
                    .target_layout()
                    .dsym_bundle_of_build_target(
                        self.packages,
                        &target,
                        self.opt.artifact_path_options().executable,
                    ),
            ],
            BuildAction::RunPrebuild { info } => info.resolved_outputs.clone(),
            BuildAction::RunMoonLexPrebuild { output, .. }
            | BuildAction::RunMoonYaccPrebuild { output, .. } => vec![output.to_path_buf()],
            _ => Vec::new(),
        };
        let semantic_outputs = action_artifacts
            .outputs
            .into_iter()
            .map(|artifact| (artifact.artifact, artifact.paths))
            .collect::<Vec<_>>();
        let outputs = semantic_outputs
            .iter()
            .flat_map(|(_, paths)| paths.iter().cloned())
            .chain(declared_outputs)
            .collect();
        let execution_action = ExecutionAction::new(
            inputs,
            outputs,
            command,
            self.string_id(action_key),
            self.human_desc(action_key, action),
        )
        .with_cache_eligible(cache_eligible)
        .with_can_dirty_on_output(matches!(
            action_key,
            BuildPlanActionKey::Backend(
                BuildPlanNode::Check(_) | BuildPlanNode::EmitProof(_) | BuildPlanNode::Prove(_)
            )
        ))
        .with_error_package(error_package);

        Ok(execution.add_action(execution_action, semantic_outputs))
    }
}
