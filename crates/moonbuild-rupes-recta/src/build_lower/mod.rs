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

//! Lowers the normalized action plan into concrete actions, then adapts them
//! to `n2`'s build graph.

use std::{collections::BTreeMap, path::PathBuf, str::FromStr, sync::OnceLock};

use log::{debug, info};
use moonutil::{
    build_options::RunMode, compiler_flags::CompilerPaths, cond_expr::OptLevel,
    target::TargetBackend,
};
use n2::graph::Graph as N2Graph;
use tracing::instrument;

use crate::{
    ResolveOutput,
    build_action_plan::{BuildActionId, BuildActionPlan},
    model::{BackendConfig, OperatingSystem, PackageId},
    pkg_name::OptionalPackageFQNWithSource,
    target_layout::{
        ArtifactPathOptions, ArtifactPathResolver, ExecutableArtifact, LinkedCoreArtifact,
    },
};

mod backend;
mod compiler;
mod context;
mod lower_aux;
mod lower_build;
mod lowered_action;
mod moonc_command;
mod n2_adapter;
mod utils;

pub use lowered_action::{
    LoweredAction, LoweredCommand, LoweredCommandExecution, LoweredProduct, LoweredResponseFile,
};
pub use utils::{build_ins, build_n2_fileloc, build_outs};

pub(crate) use backend::{CExecutableRealization, CStubLibraryRealization};

use context::LoweringContext;
use lowered_action::{BuildCommand, LoweredCommandKind};
use n2_adapter::N2GraphBuilder;

/// Lazily resolved host/toolchain facts used during lowering.
///
/// The build pipeline passes this object explicitly so lower phases do not
/// rediscover environment facts in place. Individual facts remain lazy because
/// non-native backends do not need native OS/toolchain details.
#[derive(Default)]
pub struct LoweringEnvironment {
    os: OnceLock<OperatingSystem>,
    compiler_paths: OnceLock<CompilerPaths>,
}

impl Clone for LoweringEnvironment {
    fn clone(&self) -> Self {
        let cloned = Self::default();
        if let Some(os) = self.os.get() {
            let _ = cloned.os.set(*os);
        }
        if let Some(compiler_paths) = self.compiler_paths.get() {
            let _ = cloned.compiler_paths.set(compiler_paths.clone());
        }
        cloned
    }
}

impl LoweringEnvironment {
    pub fn os(&self) -> OperatingSystem {
        *self
            .os
            .get_or_init(|| OperatingSystem::from_str(std::env::consts::OS).expect("Unknown"))
    }

    pub fn compiler_paths(&self) -> &CompilerPaths {
        self.compiler_paths
            .get_or_init(CompilerPaths::from_moon_dirs)
    }
}

/// Knobs to tweak during build. Affects behaviors during lowering.
pub struct BuildOptions {
    pub artifact_paths: ArtifactPathResolver,
    // FIXME: This overlaps with `crate::build_plan::BuildEnvironment`
    pub backend: BackendConfig,
    pub opt_level: OptLevel,
    pub action: RunMode,

    // Detailed configuration -- some of them might live better in configs
    pub debug_symbols: bool,
    pub enable_coverage: bool,
    pub moonc_output_json: bool,
    pub docs_serve: bool,
    pub warning_condition: WarningCondition,
    pub info_no_alias: bool,

    // Environments
    /// Only `Some` if we import standard library.
    pub stdlib_path: Option<PathBuf>,
    pub lowering_environment: LoweringEnvironment,
}

impl BuildOptions {
    pub fn target_backend(&self) -> TargetBackend {
        self.backend.target_backend()
    }

    pub fn os(&self) -> OperatingSystem {
        self.lowering_environment.os()
    }

    pub fn compiler_paths(&self) -> &CompilerPaths {
        self.lowering_environment.compiler_paths()
    }

    pub fn artifact_path_options(&self) -> ArtifactPathOptions {
        let os = match &self.backend {
            BackendConfig::Wasm { .. } | BackendConfig::WasmGc { .. } | BackendConfig::Js => {
                OperatingSystem::None
            }
            BackendConfig::Native(_) | BackendConfig::Llvm => self.os(),
        };
        let (executable, linked_core) = match &self.backend {
            BackendConfig::Wasm { use_wat, .. } => (
                ExecutableArtifact::Wasm { use_wat: *use_wat },
                LinkedCoreArtifact::Wasm { use_wat: *use_wat },
            ),
            BackendConfig::WasmGc { use_wat } => (
                ExecutableArtifact::WasmGC { use_wat: *use_wat },
                LinkedCoreArtifact::WasmGC { use_wat: *use_wat },
            ),
            BackendConfig::Js => (ExecutableArtifact::Js, LinkedCoreArtifact::Js),
            BackendConfig::Native(mode) => (
                if mode.tcc_run().is_some() {
                    ExecutableArtifact::TccRunResponseFile
                } else {
                    ExecutableArtifact::NativeExecutable
                },
                if mode.direct_target().is_some() {
                    LinkedCoreArtifact::NativeObject { os }
                } else {
                    LinkedCoreArtifact::NativeC
                },
            ),
            BackendConfig::Llvm => (
                ExecutableArtifact::LlvmExecutable,
                LinkedCoreArtifact::LlvmObject { os },
            ),
        };

        ArtifactPathOptions {
            os,
            executable,
            linked_core,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WarningCondition {
    /// The default behavior: warnings are shown.
    Default,
    /// Deny all warnings: treat warnings as errors.
    Deny,
    /// Allow all warnings: do not show any warnings.
    Allow,
}

/// An error that may be raised during action plan lowering.
#[derive(thiserror::Error, Debug)]
pub enum LoweringError {
    #[error("moonc response files cannot represent argument {index}: {reason}")]
    MooncResponseFile { index: usize, reason: &'static str },

    #[error(
        "An error was reported by n2 (the build graph executor), \
        when lowering for package {package}, action {action:?}"
    )]
    N2 {
        package: OptionalPackageFQNWithSource,
        action: BuildActionId,
        source: anyhow::Error,
    },
}

/// Structured command argv keyed by each generated output path.
pub type CommandArgMap = BTreeMap<PathBuf, Vec<String>>;

pub struct LoweringResult {
    /// The lowered n2 build graph.
    pub build_graph: N2Graph,

    /// Structured argv for lowered commands that are represented as argument
    /// vectors before they are rendered into n2 command strings.
    pub command_args_by_output: CommandArgMap,

    /// Artifacts corresponding to the root input actions, in input action order.
    pub artifacts: Vec<(BuildActionId, Vec<PathBuf>)>,
}

/// Lowers a normalized action plan into an n2 [Build Graph](n2::graph::Graph).
#[instrument(skip_all)]
pub fn lower_build_plan(
    resolve_output: &ResolveOutput,
    plan: &BuildActionPlan<'_>,
    opt: &BuildOptions,
) -> Result<LoweringResult, LoweringError> {
    info!("Starting action plan lowering to n2 graph");
    debug!(
        "Build options: backend={:?}, opt_level={:?}, debug_symbols={}",
        opt.target_backend(),
        opt.opt_level,
        opt.debug_symbols
    );

    let result = lower_actions(
        resolve_output,
        plan,
        opt,
        plan.action_ids(),
        plan.input_action_ids(),
    )?;

    info!("Action plan lowering completed successfully");
    Ok(result)
}

/// Project one standalone action plan into dependency and script n2 graphs.
///
/// The dependency graph contains every prerequisite outside the synthesized
/// script package, including package-less shared actions reached by those
/// prerequisites. The script graph keeps the same action/product edges; when a
/// dependency producer is omitted, its output path remains a concrete n2 input.
#[instrument(skip_all)]
pub(crate) fn lower_standalone_build_plan(
    resolve_output: &ResolveOutput,
    plan: &BuildActionPlan<'_>,
    opt: &BuildOptions,
    script_package: PackageId,
) -> Result<(LoweringResult, LoweringResult), LoweringError> {
    info!("Projecting standalone actions to dependency and script n2 graphs");
    let (dependency_actions, script_actions) = plan.partition_standalone_actions(script_package);
    debug!(
        "Standalone execution projection contains {} dependency actions and {} script actions",
        dependency_actions.len(),
        script_actions.len()
    );

    let dependencies = lower_actions(resolve_output, plan, opt, dependency_actions, &[])?;
    let script = lower_actions(
        resolve_output,
        plan,
        opt,
        script_actions,
        plan.input_action_ids(),
    )?;
    Ok((dependencies, script))
}

fn lower_actions(
    resolve_output: &ResolveOutput,
    plan: &BuildActionPlan<'_>,
    opt: &BuildOptions,
    actions: impl IntoIterator<Item = BuildActionId>,
    artifact_actions: &[BuildActionId],
) -> Result<LoweringResult, LoweringError> {
    let mut ctx = LoweringContext::new(opt.artifact_paths.clone(), resolve_output, plan, opt);
    let mut n2 = N2GraphBuilder::new();

    for id in actions {
        debug!("Lowering action: {:?}", id);
        if let Some(action) = ctx.lower_action(id)? {
            n2.add_action(action)?;
        }
    }

    let mut out_artifacts = Vec::with_capacity(artifact_actions.len());
    for &action in artifact_actions {
        let artifacts = ctx.output_paths_for_action(action);
        out_artifacts.push((action, artifacts));
    }

    Ok(LoweringResult {
        build_graph: n2.graph,
        command_args_by_output: n2.command_args_by_output,
        artifacts: out_artifacts,
    })
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, path::PathBuf};

    use indexmap::IndexSet;
    use moonutil::{
        compiler_flags::{ARKind, CC, CCKind, MsvcEnvironment, Toolchain},
        manifest::MoonMod,
        package::{MoonPkg, MoonPkgFormatter, SupportedTargetsDeclKind},
        resolution::{DEFAULT_VERSION, DirSyncResult, ModuleName, ModuleSource, ResolvedEnv},
        target::TargetBackend,
    };
    use slotmap::KeyData;

    use crate::{
        build_plan::{
            BuildCStubsInfo, BuildPlan, BuildRuntimeInfo, BuildTargetInfo, FileDependencyKind,
            LinkCoreInfo, MakeExecutableInfo, PlanArtifactNeed,
        },
        discover::{DiscoverResult, DiscoveredPackage},
        model::{
            BackendConfig, BuildPlanNode, BuildTarget, DirectNativeMode, NativeBackendMode,
            NativeTarget, TargetKind,
        },
        pkg_name::{PackageFQN, PackagePath},
        pkg_solve::DepRelationship,
        resolve::ResolveOutput,
        target_layout::{ArtifactPathResolver, ExecutableArtifact, TargetLayout, TargetLayoutMode},
    };

    use super::*;

    #[test]
    fn non_native_artifact_options_do_not_resolve_operating_system() {
        for backend in [
            BackendConfig::Wasm {
                use_wat: false,
                wasi_link: false,
            },
            BackendConfig::WasmGc { use_wat: false },
            BackendConfig::Js,
        ] {
            let artifact_paths = ArtifactPathResolver::new(
                TargetLayout::new(
                    PathBuf::from("_build"),
                    TargetLayoutMode::Workspace,
                    OptLevel::Debug,
                    RunMode::Build,
                ),
                None,
            );
            let options = BuildOptions {
                artifact_paths,
                backend,
                opt_level: OptLevel::Debug,
                action: RunMode::Build,
                debug_symbols: false,
                enable_coverage: false,
                moonc_output_json: false,
                docs_serve: false,
                warning_condition: WarningCondition::Default,
                info_no_alias: false,
                stdlib_path: None,
                lowering_environment: LoweringEnvironment::default(),
            };

            assert!(options.lowering_environment.os.get().is_none());
            assert_eq!(options.artifact_path_options().os, OperatingSystem::None);
            assert!(options.lowering_environment.os.get().is_none());
        }
    }

    fn module(name: &str) -> ModuleSource {
        ModuleSource::local_path(
            name.parse::<ModuleName>()
                .expect("test module name should parse"),
            PathBuf::from(format!("/tmp/{name}")),
            DEFAULT_VERSION.clone(),
        )
    }

    fn moon_mod(name: &str) -> MoonMod {
        MoonMod {
            name: name.to_string(),
            version: None,
            deps: Default::default(),
            bin_deps: None,
            readme: None,
            repository: None,
            license: None,
            keywords: None,
            description: None,
            compile_flags: None,
            link_flags: None,
            checksum: None,
            source: None,
            rule: None,
            ext: Default::default(),
            warn_list: None,
            include: None,
            exclude: None,
            preferred_target: None,
            supported_targets: None,
            scripts: None,
            __moonbit_unstable_prebuild: None,
        }
    }

    fn supported_targets() -> IndexSet<TargetBackend> {
        TargetBackend::all().iter().copied().collect()
    }

    fn moon_pkg(supported_targets: IndexSet<TargetBackend>) -> MoonPkg {
        MoonPkg {
            name: None,
            is_main: false,
            force_link: false,
            sub_package: None,
            imports: Vec::new(),
            wbtest_imports: Vec::new(),
            test_imports: Vec::new(),
            formatter: MoonPkgFormatter {
                ignore: Default::default(),
            },
            link: None,
            warn_list: None,
            proof_enabled: false,
            targets: None,
            pre_build: None,
            bin_name: None,
            bin_target: None,
            supported_targets,
            native_stub: None,
            virtual_pkg: None,
            implement: None,
            overrides: None,
            max_concurrent_tests: None,
            regex_backend: None,
            local_rules: None,
        }
    }

    fn msvc_toolchain() -> Toolchain {
        Toolchain::from_path_probe(CC {
            cc_kind: CCKind::Msvc,
            cc_path: "msvc/bin/cl.exe".to_string(),
            ar_kind: ARKind::MsvcLib,
            ar_path: "msvc/bin/lib.exe".to_string(),
            target_triple: None,
            is_env_override: false,
        })
        .with_msvc_environment(MsvcEnvironment {
            command_env: vec![
                ("INCLUDE".to_string(), "crt/include;sdk/include".to_string()),
                ("LIB".to_string(), "crt/lib;sdk/lib".to_string()),
            ],
        })
    }

    fn build_target_info() -> BuildTargetInfo {
        BuildTargetInfo {
            regular_files: Vec::new(),
            mbtp_files: Vec::new(),
            whitebox_files: Vec::new(),
            doctest_files: Vec::new(),
            warn_list: None,
            specified_no_mi: false,
            patch_file: None,
            why3_config: None,
            check_mi_against: None,
            value_tracing: false,
        }
    }

    fn single_package_resolve_output() -> (ResolveOutput, BuildTarget) {
        let module_source = module("username/hello");
        let (modules, module_id) =
            ResolvedEnv::only_one_module(module_source.clone(), moon_mod("username/hello"));
        let package_path = PackagePath::new("main").expect("test package path should parse");
        let supported_targets = supported_targets();
        let package = DiscoveredPackage {
            root_path: PathBuf::from("main"),
            module: module_id,
            fqn: PackageFQN::new(module_source, package_path.clone()),
            single_file_source_kind: None,
            manifest_path: Some(PathBuf::from("main/moon.pkg.json")),
            raw: Box::new(moon_pkg(supported_targets.clone())),
            supported_targets_decl: SupportedTargetsDeclKind::Omitted,
            effective_supported_targets: supported_targets,
            source_files: Vec::new(),
            mbt_lex_files: Vec::new(),
            mbt_yacc_files: Vec::new(),
            mbt_md_files: Vec::new(),
            mbtp_files: Vec::new(),
            c_stub_files: vec![PathBuf::from("main/native/stub.c")],
            virtual_mbti: None,
            is_stdlib: false,
        };

        let mut packages = DiscoverResult::default();
        packages.test_register_module(module_id, moon_mod("username/hello"));
        let package_id = packages.test_add_package(module_id, package_path, package);
        let mut module_dirs = DirSyncResult::default();
        module_dirs.insert(module_id, PathBuf::from("/tmp/username/hello"));

        (
            ResolveOutput {
                module_rel: modules,
                module_dirs,
                pkg_dirs: packages,
                pkg_rel: DepRelationship::default(),
            },
            package_id.build_target(TargetKind::Source),
        )
    }

    fn command_arg_has_normalized_suffix(command: &[String], suffix: &str) -> bool {
        command
            .iter()
            .any(|arg| arg.replace('\\', "/").ends_with(suffix))
    }

    #[test]
    fn standalone_projection_uses_dependency_closure_for_shared_actions() {
        let script_package = PackageId::from(KeyData::from_ffi(1));
        let dependency_package = PackageId::from(KeyData::from_ffi(2));
        let script_target = script_package.build_target(TargetKind::Source);
        let script_node = BuildPlanNode::MakeExecutable(script_target);
        let dependency_node = BuildPlanNode::ArchiveOrLinkCStubs(dependency_package);
        let runtime_node = BuildPlanNode::BuildRuntimeLib;

        let mut plan = BuildPlan::default();
        plan.test_add_node(script_node);
        plan.test_add_node(dependency_node);
        plan.test_add_node(runtime_node);
        plan.test_add_edge(script_node, dependency_node, FileDependencyKind::AllFiles);
        plan.test_add_edge(script_node, runtime_node, FileDependencyKind::AllFiles);
        plan.test_add_edge(dependency_node, runtime_node, FileDependencyKind::AllFiles);
        plan.test_insert_c_stubs_info(
            dependency_package,
            BuildCStubsInfo {
                effective_native_toolchain: msvc_toolchain(),
                cc_flags: Vec::new(),
                link_flags: Vec::new(),
            },
        );
        plan.test_insert_runtime_info(BuildRuntimeInfo {
            effective_native_toolchain: msvc_toolchain(),
            source_files: vec![PathBuf::from("runtime.c")],
            simdutf_objects: Vec::new(),
            static_archive_fingerprint: Some("runtime-test".to_string()),
        });

        let action_plan = plan.build_action_plan();
        let (dependency_actions, script_actions) =
            action_plan.partition_standalone_actions(script_package);
        let dependency_nodes = dependency_actions
            .into_iter()
            .map(|action| action_plan.build_plan_node(action))
            .collect::<HashSet<_>>();
        let script_nodes = script_actions
            .into_iter()
            .map(|action| action_plan.build_plan_node(action))
            .collect::<HashSet<_>>();

        assert_eq!(
            dependency_nodes,
            HashSet::from([dependency_node, runtime_node])
        );
        assert_eq!(script_nodes, HashSet::from([script_node]));
    }

    #[test]
    fn standalone_projection_keeps_script_only_shared_actions_with_script() {
        let script_package = PackageId::from(KeyData::from_ffi(1));
        let script_target = script_package.build_target(TargetKind::Source);
        let script_node = BuildPlanNode::MakeExecutable(script_target);
        let runtime_node = BuildPlanNode::BuildRuntimeLib;

        let mut plan = BuildPlan::default();
        plan.test_add_node(script_node);
        plan.test_add_node(runtime_node);
        plan.test_add_edge(script_node, runtime_node, FileDependencyKind::AllFiles);
        plan.test_insert_runtime_info(BuildRuntimeInfo {
            effective_native_toolchain: msvc_toolchain(),
            source_files: vec![PathBuf::from("runtime.c")],
            simdutf_objects: Vec::new(),
            static_archive_fingerprint: Some("runtime-test".to_string()),
        });

        let action_plan = plan.build_action_plan();
        let (dependency_actions, script_actions) =
            action_plan.partition_standalone_actions(script_package);
        let script_nodes = script_actions
            .into_iter()
            .map(|action| action_plan.build_plan_node(action))
            .collect::<HashSet<_>>();

        assert!(dependency_actions.is_empty());
        assert_eq!(script_nodes, HashSet::from([script_node, runtime_node]));
    }

    #[test]
    #[should_panic(expected = "depends on script action")]
    fn standalone_projection_rejects_dependency_work_requiring_script_action() {
        let script_package = PackageId::from(KeyData::from_ffi(1));
        let dependency_package = PackageId::from(KeyData::from_ffi(2));
        let script_node =
            BuildPlanNode::MakeExecutable(script_package.build_target(TargetKind::Source));
        let dependency_node = BuildPlanNode::ArchiveOrLinkCStubs(dependency_package);

        let mut plan = BuildPlan::default();
        plan.test_add_node(script_node);
        plan.test_add_node(dependency_node);
        plan.test_add_edge(dependency_node, script_node, FileDependencyKind::AllFiles);
        plan.test_insert_c_stubs_info(
            dependency_package,
            BuildCStubsInfo {
                effective_native_toolchain: msvc_toolchain(),
                cc_flags: Vec::new(),
                link_flags: Vec::new(),
            },
        );

        let action_plan = plan.build_action_plan();
        action_plan.partition_standalone_actions(script_package);
    }

    #[test]
    fn lowered_windows_msvc_native_exe_command_contains_complete_link_shape() {
        let (resolve_output, target) = single_package_resolve_output();
        let runtime_node = BuildPlanNode::BuildRuntimeLib;
        let runtime_object_node = BuildPlanNode::BuildRuntimeObject(0);
        let c_stub_node = BuildPlanNode::BuildCStub(target.package, 0);
        let c_stubs_node = BuildPlanNode::ArchiveOrLinkCStubs(target.package);
        let build_core_node = BuildPlanNode::BuildCore(target);
        let link_core_node = BuildPlanNode::LinkCore(target);
        let exe_node = BuildPlanNode::MakeExecutable(target);
        let toolchain = msvc_toolchain();

        let mut plan = BuildPlan::default();
        plan.test_add_node(runtime_node);
        plan.test_add_node(runtime_object_node);
        plan.test_add_node(c_stub_node);
        plan.test_add_node(c_stubs_node);
        plan.test_add_node(build_core_node);
        plan.test_add_node(link_core_node);
        plan.test_add_node(exe_node);
        plan.test_add_edge(c_stubs_node, c_stub_node, FileDependencyKind::AllFiles);
        plan.test_add_edge(
            runtime_node,
            runtime_object_node,
            FileDependencyKind::AllFiles,
        );
        plan.test_add_edge(
            link_core_node,
            build_core_node,
            FileDependencyKind::Artifacts(PlanArtifactNeed::CoreIr),
        );
        plan.test_add_edge(exe_node, link_core_node, FileDependencyKind::AllFiles);
        plan.test_add_edge(exe_node, runtime_node, FileDependencyKind::AllFiles);
        plan.test_add_edge(exe_node, c_stubs_node, FileDependencyKind::AllFiles);
        plan.test_insert_build_target_info(target, build_target_info());
        plan.test_insert_link_core_info(
            target,
            LinkCoreInfo {
                linked_order: vec![target],
                abort_overridden: false,
            },
        );
        plan.test_insert_c_stubs_info(
            target.package,
            BuildCStubsInfo {
                effective_native_toolchain: toolchain.clone(),
                cc_flags: Vec::new(),
                link_flags: Vec::new(),
            },
        );
        plan.test_insert_runtime_info(BuildRuntimeInfo {
            effective_native_toolchain: toolchain.clone(),
            source_files: vec![PathBuf::from("runtime.c")],
            simdutf_objects: Vec::new(),
            static_archive_fingerprint: Some("runtime-test".to_string()),
        });
        plan.test_insert_make_executable_info(
            target,
            MakeExecutableInfo {
                effective_native_toolchain: toolchain.clone(),
                c_flags: Vec::new(),
                link_flags: vec!["dep.lib".to_string(), "/LIBPATH:pkg/lib".to_string()],
                link_c_stubs: vec![target.package],
            },
        );

        let lowering_environment = LoweringEnvironment::default();
        lowering_environment
            .os
            .set(OperatingSystem::Windows)
            .expect("test OS should be set once");
        let artifact_paths = ArtifactPathResolver::new(
            TargetLayout::new(
                PathBuf::from("_build"),
                TargetLayoutMode::Workspace,
                OptLevel::Debug,
                RunMode::Build,
            ),
            None,
        );
        let native_mode = NativeBackendMode::DirectObject(DirectNativeMode::Target(
            NativeTarget::X86_64PcWindowsMsvc,
        ));
        let options = BuildOptions {
            artifact_paths: artifact_paths.clone(),
            backend: BackendConfig::Native(native_mode),
            opt_level: OptLevel::Debug,
            action: RunMode::Build,
            debug_symbols: false,
            enable_coverage: false,
            moonc_output_json: false,
            docs_serve: false,
            warning_condition: WarningCondition::Default,
            info_no_alias: false,
            stdlib_path: None,
            lowering_environment,
        };

        let action_plan = plan.build_action_plan();
        let lowered = lower_build_plan(&resolve_output, &action_plan, &options)
            .expect("lowering should succeed");
        let exe_path = artifact_paths.target_layout().executable_of_build_target(
            &resolve_output.pkg_dirs,
            &target,
            ExecutableArtifact::NativeExecutable,
        );
        let command = lowered
            .command_args_by_output
            .get(&exe_path)
            .expect("executable command args should be captured");

        assert!(command.iter().any(|arg| arg == "msvc/bin/cl.exe"));
        assert!(command.iter().any(|arg| arg == "/subsystem:console"));
        assert!(command.iter().any(|arg| arg == "/LIBPATH:pkg/lib"));
        assert!(command.iter().any(|arg| arg == "dep.lib"));
        assert!(command.iter().any(|arg| arg == "libcmt.lib"));
        assert!(command.iter().any(|arg| arg == "kernel32.lib"));
        assert!(command_arg_has_normalized_suffix(
            command,
            "username/hello/main/libmain.lib"
        ));
        assert!(command_arg_has_normalized_suffix(
            command,
            "native/debug/build/libruntime-runtime-test.lib"
        ));
        let c_stub_position = command
            .iter()
            .position(|arg| {
                arg.replace('\\', "/")
                    .ends_with("username/hello/main/libmain.lib")
            })
            .expect("C stub archive should be linked");
        let runtime_position = command
            .iter()
            .position(|arg| {
                arg.replace('\\', "/")
                    .ends_with("native/debug/build/libruntime-runtime-test.lib")
            })
            .expect("runtime archive should be linked");
        assert!(c_stub_position < runtime_position);

        let runtime_compile_command = lowered
            .command_args_by_output
            .values()
            .find(|command| command_arg_has_normalized_suffix(command, "runtime.c"))
            .expect("runtime compile command args should be captured");
        assert!(command_arg_has_normalized_suffix(
            runtime_compile_command,
            "native/debug/build/runtime-runtime.obj"
        ));

        let runtime_archive_command = lowered
            .command_args_by_output
            .values()
            .find(|command| {
                command.iter().any(|arg| arg == "msvc/bin/lib.exe")
                    && command_arg_has_normalized_suffix(
                        command,
                        "native/debug/build/libruntime-runtime-test.lib",
                    )
            })
            .expect("runtime archive command args should be captured");
        assert!(command_arg_has_normalized_suffix(
            runtime_archive_command,
            "native/debug/build/libruntime-runtime-test.lib"
        ));
        assert!(command_arg_has_normalized_suffix(
            runtime_archive_command,
            "native/debug/build/runtime-runtime.obj"
        ));

        let stub_compile_command = lowered
            .command_args_by_output
            .values()
            .find(|command| command_arg_has_normalized_suffix(command, "main/native/stub.c"))
            .expect("C stub compile command args should be captured");
        assert!(
            stub_compile_command
                .iter()
                .any(|arg| arg == moonutil::compiler_flags::WINDOWS_MSVC_STATIC_RUNTIME_FLAG)
        );

        let msvc_env_build = lowered
            .build_graph
            .builds
            .iter()
            .find(|build| build.env.iter().any(|(key, _)| key == "INCLUDE"))
            .expect("MSVC build should carry command environment");
        assert!(
            msvc_env_build
                .env
                .iter()
                .any(|(key, value)| key == "LIB" && value == "crt/lib;sdk/lib")
        );
    }
}
