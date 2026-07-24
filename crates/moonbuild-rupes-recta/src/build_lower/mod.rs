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

//! Lowers the normalized action plan into `n2`'s build graph.

use std::{collections::BTreeMap, path::PathBuf, str::FromStr, sync::OnceLock};

use log::{debug, info};
use moonutil::{
    build_options::RunMode,
    compiler_flags::{CompilerPaths, Toolchain},
    cond_expr::OptLevel,
};
use n2::graph::{Graph as N2Graph, RspFile};
use tracing::instrument;

use crate::{
    ResolveOutput,
    build_action_plan::{BuildActionId, BuildActionPlan},
    model::{NativeBackendMode, OperatingSystem, RunBackend},
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
mod moonc_command;
mod utils;

pub use utils::{build_ins, build_n2_fileloc, build_outs};

pub(crate) use backend::{CExecutableRealization, CStubLibraryRealization, SelectedBackend};

use context::LoweringContext;

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

    pub fn runtime_dot_c_path(&self) -> PathBuf {
        PathBuf::from(&self.compiler_paths().lib_path).join("runtime.c")
    }
}

/// Knobs to tweak during build. Affects behaviors during lowering.
pub struct BuildOptions {
    pub artifact_paths: ArtifactPathResolver,
    // FIXME: This overlaps with `crate::build_plan::BuildEnvironment`
    pub target_backend: RunBackend,
    pub native_mode: NativeBackendMode,
    pub(crate) selected_backend: SelectedBackend,
    pub opt_level: OptLevel,
    pub action: RunMode,

    // Detailed configuration -- some of them might live better in configs
    pub debug_symbols: bool,
    pub enable_coverage: bool,
    pub output_wat: bool,
    pub moonc_output_json: bool,
    pub docs_serve: bool,
    pub warning_condition: WarningCondition,
    pub info_no_alias: bool,
    pub wasi_link: bool,
    pub collect_dependency_build_actions: bool,

    // Environments
    /// Only `Some` if we import standard library.
    pub stdlib_path: Option<PathBuf>,
    pub lowering_environment: LoweringEnvironment,
}

impl BuildOptions {
    pub fn os(&self) -> OperatingSystem {
        self.lowering_environment.os()
    }

    pub fn compiler_paths(&self) -> &CompilerPaths {
        self.lowering_environment.compiler_paths()
    }

    pub fn runtime_dot_c_path(&self) -> PathBuf {
        self.lowering_environment.runtime_dot_c_path()
    }

    pub fn use_tcc_run(&self) -> bool {
        let use_tcc_run = self.native_mode.is_tcc_run();
        debug_assert!(!use_tcc_run || self.target_backend == RunBackend::Native);
        debug_assert!(!use_tcc_run || self.native_mode.direct_target().is_none());
        use_tcc_run
    }

    pub fn artifact_path_options(&self) -> ArtifactPathOptions {
        let use_tcc_run = self.use_tcc_run();
        let os = match self.target_backend {
            RunBackend::Wasm | RunBackend::WasmGC | RunBackend::Js => OperatingSystem::None,
            RunBackend::Native | RunBackend::Llvm => self.os(),
        };
        let executable = match self.target_backend {
            RunBackend::Wasm => ExecutableArtifact::Wasm {
                use_wat: self.selected_backend.use_wat(),
            },
            RunBackend::WasmGC => ExecutableArtifact::WasmGC {
                use_wat: self.selected_backend.use_wat(),
            },
            RunBackend::Js => ExecutableArtifact::Js,
            RunBackend::Native if use_tcc_run => ExecutableArtifact::TccRunResponseFile,
            RunBackend::Native => ExecutableArtifact::NativeExecutable,
            RunBackend::Llvm => ExecutableArtifact::LlvmExecutable,
        };
        let linked_core = match self.target_backend {
            RunBackend::Wasm => LinkedCoreArtifact::Wasm {
                use_wat: self.selected_backend.use_wat(),
            },
            RunBackend::WasmGC => LinkedCoreArtifact::WasmGC {
                use_wat: self.selected_backend.use_wat(),
            },
            RunBackend::Js => LinkedCoreArtifact::Js,
            RunBackend::Native if self.native_mode.direct_target().is_some() => {
                LinkedCoreArtifact::NativeObject { os }
            }
            RunBackend::Native => LinkedCoreArtifact::NativeC,
            RunBackend::Llvm => LinkedCoreArtifact::LlvmObject { os },
        };

        ArtifactPathOptions {
            target_backend: self.target_backend,
            use_tcc_run,
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

    /// Registry dependency `BuildCore` actions that can be prepared as one
    /// standalone-script dependency graph.
    pub dependency_build_actions: Vec<crate::dependency_build_cache::DependencyBuildAction>,

    /// Artifacts corresponding to the root input actions, in input action order.
    pub artifacts: Vec<(BuildActionId, Vec<PathBuf>)>,
}

/// The command to execute for n2.
///
/// # How n2 handles commandlines
///
/// N2 (and ninja) use different conventions for handling commandlines on
/// different platforms.
///
/// - On Unix-like platforms, the command string will be fed into `sh -c`. Thus,
///   shell features like variable expansion are supported.
/// - On Windows, the command string will be directly passed to
///   `CreateProcessA`. No shell features are supported.
///
/// For most build commands, this is not an issue. All executables and argument
/// paths are absolute paths, and there's no shell features involved.
///
/// However, for prebuild commands, the commandline is expected to be copied
/// verbatim (with minimal resolving) to the generated build script. Thus,
/// splitting, resolving and quoting again may lead to e.g. shell features being
/// lost.
///
/// Thus, we're currently providing a `Verbatim` variant to handle such cases.
///
/// # Future improvements
///
/// Future design might want to omit shell features entirely for better
/// cross-platform consistency. Env var expansion are already used by some
/// libraries, so maintainers must be careful not to break those while doing so.
///
/// An idea is to use unix-style shell splitting and expansion everywhere,
/// performing the env var expansion ourselves during build graph execution
/// time. Other shell features should be disallowed. The result will then be
/// handled like `Args` native to the platform.
#[derive(Debug, Clone)]
enum CommandlineKind {
    /// This commandline will be joined using the platform's default convention.
    Args(Vec<String>),

    /// This verbatim string will be plugged into the build graph as-is.
    /// Use with caution.
    ///
    /// This variant is used for commands that intentionally rely on shell
    /// composition, such as prebuild commands and follow-up tool invocations.
    Verbatim,
}

/// How n2 should execute a logical command.
#[derive(Debug, Clone)]
enum CommandExecution {
    Inline(String),
    ResponseFile { command: String, file: RspFile },
}

#[derive(Debug, Clone)]
struct Commandline {
    /// Structured logical argv, when available for metadata and presentation.
    kind: CommandlineKind,
    execution: CommandExecution,
    cwd: Option<PathBuf>,
    env: Vec<(String, String)>,
}

impl From<Vec<String>> for Commandline {
    fn from(v: Vec<String>) -> Self {
        let command = moonutil::shlex::join_native(v.iter().map(String::as_str));
        Commandline {
            kind: CommandlineKind::Args(v),
            execution: CommandExecution::Inline(command),
            cwd: None,
            env: Vec::new(),
        }
    }
}

impl Commandline {
    fn verbatim(s: String) -> Self {
        Self {
            kind: CommandlineKind::Verbatim,
            execution: CommandExecution::Inline(s),
            cwd: None,
            env: Vec::new(),
        }
    }

    fn into_n2(self) -> (String, Option<RspFile>) {
        match self.execution {
            CommandExecution::Inline(command) => (command, None),
            CommandExecution::ResponseFile { command, file } => (command, Some(file)),
        }
    }

    fn inline_command(&self) -> &str {
        let CommandExecution::Inline(command) = &self.execution else {
            unreachable!("a response-file command is already lowered")
        };
        command
    }

    fn with_response_file(mut self, command: String, file: RspFile) -> Self {
        self.execution = CommandExecution::ResponseFile { command, file };
        self
    }

    fn args(&self) -> Option<&Vec<String>> {
        match &self.kind {
            CommandlineKind::Args(args) => Some(args),
            CommandlineKind::Verbatim => None,
        }
    }

    fn with_cwd(mut self, cwd: PathBuf) -> Self {
        self.cwd = Some(cwd);
        self
    }

    fn with_env(mut self, env: Vec<(String, String)>) -> Self {
        self.env.extend(env);
        self
    }
}

/// Represents the essential information needed to construct an [`Build`] value
/// that cannot be derived fromthe build plan graph.
struct BuildCommand {
    /// The **extra** input files needed for this command, **in addition to**
    /// the artifacts of the build steps this command depends on.
    extra_inputs: Vec<PathBuf>,

    /// The command to execute.
    commandline: Commandline,
}

impl BuildCommand {
    fn with_cwd(mut self, cwd: PathBuf) -> Self {
        self.commandline = self.commandline.with_cwd(cwd);
        self
    }

    fn with_env(mut self, env: Vec<(String, String)>) -> Self {
        self.commandline = self.commandline.with_env(env);
        self
    }

    fn with_msvc_env(self, toolchain: &Toolchain) -> Self {
        self.with_env(compiler::msvc::command_env(toolchain))
    }
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
        opt.target_backend, opt.opt_level, opt.debug_symbols
    );

    let mut ctx = LoweringContext::new(opt.artifact_paths.clone(), resolve_output, plan, opt);

    for id in plan.action_ids() {
        debug!("Lowering action: {:?}", id);
        ctx.lower_action(id)?;
    }

    let mut out_artifacts = Vec::with_capacity(plan.input_action_ids().len());
    for &action in plan.input_action_ids() {
        let artifacts = ctx.output_paths_for_action(action);
        out_artifacts.push((action, artifacts));
    }

    info!("Action plan lowering completed successfully");
    Ok(LoweringResult {
        build_graph: ctx.graph,
        command_args_by_output: ctx.command_args_by_output,
        dependency_build_actions: ctx.dependency_build_actions,
        artifacts: out_artifacts,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use indexmap::IndexSet;
    use moonutil::{
        compiler_flags::{ARKind, CC, CCKind, MsvcEnvironment, Toolchain},
        manifest::MoonMod,
        package::{MoonPkg, MoonPkgFormatter, SupportedTargetsDeclKind},
        resolution::{DEFAULT_VERSION, DirSyncResult, ModuleName, ModuleSource, ResolvedEnv},
        target::TargetBackend,
    };

    use crate::{
        build_plan::{
            BuildCStubsInfo, BuildPlan, BuildRuntimeInfo, BuildTargetInfo, FileDependencyKind,
            LinkCoreInfo, MakeExecutableInfo, PlanArtifactNeed,
        },
        discover::{DiscoverResult, DiscoveredPackage},
        model::{
            BuildPlanNode, BuildTarget, DirectNativeMode, NativeBackendMode, NativeTarget,
            TargetKind,
        },
        pkg_name::{PackageFQN, PackagePath},
        pkg_solve::DepRelationship,
        resolve::ResolveOutput,
        target_layout::{ArtifactPathResolver, ExecutableArtifact, TargetLayout, TargetLayoutMode},
    };

    use super::*;

    #[test]
    fn non_native_artifact_options_do_not_resolve_operating_system() {
        for target_backend in [RunBackend::Wasm, RunBackend::WasmGC, RunBackend::Js] {
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
                target_backend,
                native_mode: NativeBackendMode::GeneratedC,
                selected_backend: SelectedBackend::new(
                    target_backend,
                    &NativeBackendMode::GeneratedC,
                    false,
                ),
                opt_level: OptLevel::Debug,
                action: RunMode::Build,
                debug_symbols: false,
                enable_coverage: false,
                output_wat: false,
                moonc_output_json: false,
                docs_serve: false,
                warning_condition: WarningCondition::Default,
                info_no_alias: false,
                wasi_link: false,
                collect_dependency_build_actions: false,
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
            is_single_file: false,
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
    fn lowered_windows_msvc_native_exe_command_contains_complete_link_shape() {
        let (resolve_output, target) = single_package_resolve_output();
        let runtime_node = BuildPlanNode::BuildRuntimeLib;
        let c_stub_node = BuildPlanNode::BuildCStub(target.package, 0);
        let c_stubs_node = BuildPlanNode::ArchiveOrLinkCStubs(target.package);
        let build_core_node = BuildPlanNode::BuildCore(target);
        let link_core_node = BuildPlanNode::LinkCore(target);
        let exe_node = BuildPlanNode::MakeExecutable(target);
        let toolchain = msvc_toolchain();

        let mut plan = BuildPlan::default();
        plan.test_add_node(runtime_node);
        plan.test_add_node(c_stub_node);
        plan.test_add_node(c_stubs_node);
        plan.test_add_node(build_core_node);
        plan.test_add_node(link_core_node);
        plan.test_add_node(exe_node);
        plan.test_add_edge(c_stubs_node, c_stub_node, FileDependencyKind::AllFiles);
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
            target_backend: RunBackend::Native,
            native_mode: native_mode.clone(),
            selected_backend: SelectedBackend::new(RunBackend::Native, &native_mode, false),
            opt_level: OptLevel::Debug,
            action: RunMode::Build,
            debug_symbols: false,
            enable_coverage: false,
            output_wat: false,
            moonc_output_json: false,
            docs_serve: false,
            warning_condition: WarningCondition::Default,
            info_no_alias: false,
            wasi_link: false,
            collect_dependency_build_actions: false,
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
