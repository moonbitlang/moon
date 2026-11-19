// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use moonutil::cond_expr::OptLevel;

use crate::{
    build_lower::WarningCondition,
    model::{BackendConfig, DebugInfoRequest},
    target_layout::ArtifactPathResolver,
};

/// Build operation selecting planning rules and the target-directory layout.
#[derive(Debug, Clone, PartialEq, Copy)]
pub enum RunMode {
    Bench,
    Build,
    Check,
    Prove,
    Run,
    Test,
    Bundle,
    Format,
}

impl RunMode {
    pub fn to_dir_name(self) -> &'static str {
        match self {
            Self::Bench => "bench",
            Self::Build | Self::Run => "build",
            Self::Check => "check",
            Self::Prove => "prove",
            Self::Test => "test",
            Self::Bundle => "bundle",
            Self::Format => "format",
        }
    }
}

/// Build configuration shared by planning and lowering.
pub struct CompileConfig {
    /// The backend and backend-specific configuration selected for this build.
    pub backend: BackendConfig,
    /// The optimization level to use for the compilation.
    pub opt_level: OptLevel,
    /// The build operation selecting planning rules and the target-directory layout.
    pub action: RunMode,
    /// Debug information requested before the Native payload form is selected.
    pub debug_info: DebugInfoRequest,

    /// Physical artifact paths, including the selected target directory and
    /// optional installed standard-library root.
    pub artifact_paths: ArtifactPathResolver,
    // MAINTAINERS: consider moving some of these to per-package/module options.
    /// Whether to export the build plan graph in the compile output.
    /// This should only be used in debugging scenarios.
    pub debug_export_build_plan: bool,
    /// Enable code coverage instrumentation.
    pub enable_coverage: bool,
    /// Whether to output JSON or human-readable error code
    pub moonc_output_json: bool,
    /// Whether to output HTML for docs (in serve mode)
    pub docs_serve: bool,
    /// Whether to disallow all warnings
    pub warning_condition: WarningCondition,
    /// List of warnings to enable
    pub warn_list: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::RunMode;
    use moonutil::{
        cond_expr::OptLevel,
        manifest::MoonMod,
        module_name::ModuleName,
        package::{MoonPkg, MoonPkgFormatter, SupportedTargetsDeclKind},
        resolution::{DEFAULT_VERSION, DirSyncResult, ModuleDependencyGraph, ModuleSource},
        target::TargetBackend,
        user_log::UserLog,
    };

    use crate::{
        CompileConfig, ResolvedProject,
        build_lower::{WarningCondition, lower_build_plan},
        build_plan::{ArtifactKey, InputDirective, build_plan},
        discover::{DiscoverResult, DiscoveredPackage, SingleFileSourceKind},
        model::{
            BackendConfig, BuildPlanNode, DebugInfoRequest, DebugSymbols, NativeTarget, TargetKind,
        },
        pkg_name::{PackageFQN, PackagePath},
        pkg_solve::{DepEdge, PackageRelations},
        target_layout::{ArtifactPathResolver, TargetLayout, TargetLayoutMode},
    };

    fn moon_mod() -> MoonMod {
        MoonMod {
            name: "test/single".to_string(),
            version: None,
            private: None,
            deps: Default::default(),
            readme: None,
            repository: None,
            license: None,
            keywords: None,
            description: None,
            compile_flags: None,
            link_flags: None,
            source: None,
            rule: None,
            ext: Default::default(),
            warn_list: None,
            preferred_target: None,
            supported_targets: None,
            scripts: None,
            __moonbit_unstable_prebuild: None,
        }
    }

    fn moon_pkg(is_main: bool) -> MoonPkg {
        MoonPkg {
            name: None,
            experimental: false,
            allow_experimental_packages: None,
            is_main,
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
            alert_list: None,
            proof_enabled: false,
            targets: None,
            pre_build: None,
            supported_targets: TargetBackend::all().iter().copied().collect(),
            native_stub: None,
            virtual_pkg: None,
            implement: None,
            overrides: None,
            max_concurrent_tests: None,
            regex_backend: None,
            local_rules: None,
        }
    }

    fn package(
        module: moonutil::resolution::ModuleId,
        module_source: &ModuleSource,
        path: &str,
        is_single_file: bool,
        is_main: bool,
    ) -> DiscoveredPackage {
        let package_path = PackagePath::new(path).expect("test package path should parse");
        DiscoveredPackage {
            root_path: PathBuf::from(path),
            module,
            fqn: PackageFQN::new(module_source.clone(), package_path),
            single_file_source_kind: is_single_file.then_some(SingleFileSourceKind::Mbt),
            manifest_path: (!is_single_file).then(|| PathBuf::from(path).join("moon.pkg.json")),
            raw: Box::new(moon_pkg(is_main)),
            supported_targets_decl: SupportedTargetsDeclKind::Omitted,
            effective_supported_targets: TargetBackend::all().iter().copied().collect(),
            source_files: vec![PathBuf::from(path).join(format!("{path}.mbt"))],
            mbt_md_files: Vec::new(),
            mbtp_files: Vec::new(),
            c_stub_files: Vec::new(),
            c_stub_header_files: Vec::new(),
            virtual_mbti_files: Vec::new(),
            is_stdlib: false,
        }
    }

    #[test]
    fn native_debug_info_is_scoped_to_each_compiler() {
        // These are plans, not cross-compilations. An explicit candidate makes
        // direct-object coverage independent of the host and MOONBIT_NEW_NATIVE.
        for (candidate, c_flags, opt_level, generated_c) in [
            (None, None, OptLevel::Debug, true),
            (
                Some(NativeTarget::X86_64UnknownLinuxGnu),
                None,
                OptLevel::Debug,
                false,
            ),
            (
                Some(NativeTarget::X86_64UnknownLinuxGnu),
                Some("-DGENERATED_C_DEBUG_TEST"),
                OptLevel::Debug,
                true,
            ),
            (None, None, OptLevel::Release, true),
        ] {
            let module_source = ModuleSource::local_path(
                "test/single"
                    .parse()
                    .expect("test module name should parse"),
                PathBuf::from("."),
                DEFAULT_VERSION.clone(),
            )
            .expect("test module should have a valid version");
            let (modules, module) =
                ModuleDependencyGraph::only_one_module(module_source.clone(), moon_mod());
            let mut packages = DiscoverResult::default();
            packages.test_register_module(module, moon_mod());
            let mut main = package(module, &module_source, "main", false, true);
            main.c_stub_files = vec![PathBuf::from("main/stub.c")];
            if let Some(flags) = c_flags {
                main.raw.link = Some(moonutil::package::Link {
                    extra: Default::default(),
                    wasm: None,
                    wasm_gc: None,
                    js: None,
                    native: Some(moonutil::package::NativeLinkConfig {
                        extra: Default::default(),
                        exports: None,
                        cc: None,
                        cc_flags: Some(flags.into()),
                        cc_link_flags: None,
                        stub_cc: None,
                        stub_cc_flags: None,
                    }),
                });
            }
            let main = packages.test_add_package(
                module,
                PackagePath::new("main").expect("main package path should parse"),
                main,
            );
            let target = main.build_target(TargetKind::Source);
            let mut relationship = PackageRelations::default();
            relationship.dep_graph.add_node(target);
            let mut module_dirs = DirSyncResult::default();
            module_dirs.insert(module, PathBuf::from("."));
            let resolved = ResolvedProject {
                discovered: crate::resolve::DiscoveredProject {
                    module_graph: modules,
                    module_dirs,
                    pkg_dirs: packages,
                    enable_coverage: false,
                },
                package_relations: moonutil::target::TargetBackend::all()
                    .iter()
                    .map(|&b| (b, relationship.clone()))
                    .collect(),
            };
            for symbols in [
                DebugSymbols::None,
                DebugSymbols::Backtrace,
                DebugSymbols::Full,
            ] {
                let config = CompileConfig {
                    backend: BackendConfig::Native {
                        direct_object_candidate: candidate,
                        collect_ref_cycle: false,
                        allocator: moonutil::compiler_flags::NativeAllocator::System,
                        os: std::env::consts::OS
                            .parse()
                            .expect("test host OS should be supported"),
                        compiler_paths: moonutil::compiler_flags::CompilerPaths::from_moon_dirs(),
                    },
                    opt_level,
                    action: RunMode::Run,
                    debug_info: DebugInfoRequest {
                        symbols,
                        runtime_backtrace: true,
                    },
                    artifact_paths: ArtifactPathResolver::new(
                        TargetLayout::new(
                            PathBuf::from("_build"),
                            TargetLayoutMode::Mono {
                                main_module: module_source.clone(),
                            },
                            opt_level,
                            RunMode::Run,
                        ),
                        None,
                    ),
                    debug_export_build_plan: false,
                    enable_coverage: false,
                    moonc_output_json: false,
                    docs_serve: false,
                    warning_condition: WarningCondition::Default,
                    warn_list: None,
                };
                let plan = build_plan(
                    &resolved,
                    &config,
                    [ArtifactKey::Executable {
                        package: main,
                        target_kind: TargetKind::Source,
                    }]
                    .into_iter(),
                    &InputDirective::default(),
                    None,
                    &UserLog::new(log::LevelFilter::Off),
                )
                .expect("native executable should plan");
                let headers = [PathBuf::from("/supplied/include/moonbit.h")];
                let execution_plan = lower_build_plan(&resolved, &plan, &config, &headers)
                    .expect("native executable should lower");
                for id in execution_plan.action_ids() {
                    let action = execution_plan.action(id);
                    let compiles_c = action.outputs().iter().any(|path| {
                        matches!(
                            execution_plan
                                .declared_output(path)
                                .and_then(|output| output.artifact()),
                            Some(
                                ArtifactKey::GeneratedCObject { .. }
                                    | ArtifactKey::CStubObject { .. }
                                    | ArtifactKey::RuntimeObject { .. }
                            )
                        )
                    });
                    assert_eq!(action.inputs().contains(&headers[0]), compiles_c);
                }
                let commands = execution_plan
                    .action_ids()
                    .map(|id| execution_plan.action(id).command().args())
                    .collect::<Vec<_>>();
                let moonc_debug = symbols == DebugSymbols::Full
                    || (symbols == DebugSymbols::Backtrace && generated_c);
                for command in ["build-package", "link-core"] {
                    let args = commands
                        .iter()
                        .find(|args| args.get(1).is_some_and(|arg| arg == command))
                        .expect("native executable should build and link MoonBit code");
                    assert_eq!(args.iter().any(|arg| arg == "-g"), moonc_debug, "{args:?}");
                    assert_eq!(
                        args.iter().any(|arg| arg == "-O0"),
                        opt_level == OptLevel::Debug,
                        "{args:?}"
                    );
                    assert_eq!(
                        args.iter().any(|arg| arg == "-stacktrace"),
                        !generated_c && symbols == DebugSymbols::Backtrace,
                        "{args:?}"
                    );
                    if command == "link-core" {
                        let output = args
                            .windows(2)
                            .find(|pair| pair[0] == "-o")
                            .expect("link-core should name its output");
                        assert_eq!(output[1].ends_with(".c"), generated_c, "{args:?}");
                        if !generated_c {
                            assert!(
                                output[1].ends_with(".o") || output[1].ends_with(".obj"),
                                "{args:?}"
                            );
                            assert!(
                                args.iter().any(|arg| arg == "x86_64-unknown-linux-gnu"),
                                "{args:?}"
                            );
                        }
                    }
                }
                let program = commands
                    .iter()
                    .find(|args| {
                        args.get(1).is_none_or(|arg| arg != "link-core")
                            && args.iter().any(|arg| {
                                matches!(
                                    Path::new(arg).file_name().and_then(|name| name.to_str()),
                                    Some("main.c") if generated_c
                                ) || (!generated_c
                                    && matches!(
                                        Path::new(arg).file_name().and_then(|name| name.to_str()),
                                        Some("main.o" | "main.obj")
                                    ))
                            })
                    })
                    .expect("native executable needs a compiler or linker action");
                if generated_c {
                    assert!(
                        program.iter().any(|arg| arg == "-c" || arg == "/c"),
                        "generated C must compile separately so compiler wrappers can cache it: {program:?}"
                    );
                    let execution = &execution_plan;
                    let link = execution
                        .action_ids()
                        .map(|id| execution.action(id))
                        .find(|action| {
                            action.outputs().iter().any(|path| {
                                matches!(
                                    execution
                                        .declared_output(path)
                                        .and_then(|output| output.artifact()),
                                    Some(ArtifactKey::Executable { .. })
                                )
                            })
                        })
                        .expect("executable must have a link action");
                    assert!(!link.command().args().iter().any(|arg| arg.ends_with(".c")));
                    assert!(
                        link.command()
                            .args()
                            .iter()
                            .any(|arg| { arg.ends_with("main.o") || arg.ends_with("main.obj") })
                    );
                    if let Some(flag) = c_flags {
                        assert!(program.iter().any(|arg| arg == flag));
                        assert!(
                            link.command().args().iter().any(|arg| arg == flag),
                            "compiler-driver flags retain their link effects"
                        );
                    }
                }
                assert_eq!(
                    program.iter().any(|arg| arg == "-g" || arg == "/Z7"),
                    generated_c && moonc_debug,
                    "{program:?}"
                );
                if generated_c && c_flags.is_none() {
                    let expected = match opt_level {
                        OptLevel::Debug => ["-Og", "/Od"],
                        OptLevel::Release => ["-O2", "/O2"],
                    };
                    assert!(
                        program.iter().any(|arg| expected.contains(&arg.as_str())),
                        "{program:?}"
                    );
                } else {
                    assert!(
                        !program
                            .iter()
                            .any(|arg| ["-O0", "-Og", "-O2", "/Od", "/O2"].contains(&arg.as_str())),
                        "link-only actions and custom C flags must not acquire compiler optimization defaults: {program:?}"
                    );
                }
                let stub = commands
                    .iter()
                    .find(|args| args.iter().any(|arg| arg.ends_with("stub.c")))
                    .expect("native executable should compile its C stub");
                assert_eq!(
                    stub.iter().any(|arg| arg == "-g" || arg == "/Z7"),
                    symbols == DebugSymbols::Full,
                    "C fallback must not enable stub debug info: {stub:?}"
                );
                let expected = match (opt_level, symbols == DebugSymbols::Full) {
                    (_, true) => ["-Og", "/Od"],
                    (OptLevel::Debug, false) => ["-O0", "/Od"],
                    (OptLevel::Release, false) => ["-O2", "/O2"],
                };
                assert!(
                    stub.iter().any(|arg| expected.contains(&arg.as_str())),
                    "{stub:?}"
                );
                let runtime = commands
                    .iter()
                    .find(|args| args.iter().any(|arg| arg.ends_with("runtime.c")))
                    .expect("native executable should compile the runtime");
                assert!(
                    runtime.iter().any(|arg| arg == "-g" || arg == "/Z7"),
                    "runtime debug info is independent: {runtime:?}"
                );
                assert!(
                    runtime.iter().any(|arg| arg == "-O2" || arg == "/O2"),
                    "runtime optimization is independent: {runtime:?}"
                );
            }
        }
    }

    #[test]
    fn native_payload_selection_uses_only_requested_executables() {
        use crate::{
            build_plan::{build_plan, resolve_native_backend_mode},
            model::{NativeTarget, OperatingSystem},
        };

        let module_source = ModuleSource::local_path(
            "test/single"
                .parse::<ModuleName>()
                .expect("test module should parse"),
            PathBuf::from("."),
            DEFAULT_VERSION.clone(),
        )
        .expect("test module should have a valid version");
        let (modules, module) =
            ModuleDependencyGraph::only_one_module(module_source.clone(), moon_mod());
        let mut packages = DiscoverResult::default();
        packages.test_register_module(module, moon_mod());
        let plain = packages.test_add_package(
            module,
            PackagePath::new("plain").expect("plain package path should parse"),
            package(module, &module_source, "plain", false, true),
        );
        let mut with_flags = package(module, &module_source, "with_flags", false, true);
        with_flags.raw.link = Some(moonutil::package::Link {
            extra: Default::default(),
            wasm: None,
            wasm_gc: None,
            js: None,
            native: Some(moonutil::package::NativeLinkConfig {
                extra: Default::default(),
                exports: None,
                cc: None,
                cc_flags: Some("-fno-inline".into()),
                cc_link_flags: None,
                stub_cc: None,
                stub_cc_flags: None,
            }),
        });
        let with_flags = packages.test_add_package(
            module,
            PackagePath::new("with_flags").expect("flagged package path should parse"),
            with_flags,
        );
        let resolved = ResolvedProject {
            discovered: crate::resolve::DiscoveredProject {
                module_graph: modules,
                module_dirs: DirSyncResult::default(),
                pkg_dirs: packages,
                enable_coverage: false,
            },
            package_relations: moonutil::target::TargetBackend::all()
                .iter()
                .map(|&b| (b, PackageRelations::default()))
                .collect(),
        };
        // Empty plans exercise capability forwarding without resolving host tools.
        // Include foreign targets and None so a planner-side host read cannot pass.
        for candidate in [
            None,
            Some(NativeTarget::Aarch64AppleDarwin),
            Some(NativeTarget::X86_64UnknownLinuxGnu),
            Some(NativeTarget::X86_64PcWindowsMsvc),
        ] {
            for (opt_level, collect_ref_cycle) in [
                (OptLevel::Debug, false),
                (OptLevel::Debug, true),
                (OptLevel::Release, false),
                (OptLevel::Release, true),
            ] {
                let config = CompileConfig {
                    backend: BackendConfig::Native {
                        direct_object_candidate: candidate,
                        collect_ref_cycle,
                        allocator: moonutil::compiler_flags::NativeAllocator::Default,
                        os: OperatingSystem::None,
                        compiler_paths: moonutil::compiler_flags::CompilerPaths {
                            include_path: "/unused/include".into(),
                            lib_path: "/unused/lib".into(),
                        },
                    },
                    opt_level,
                    action: RunMode::Build,
                    debug_info: DebugInfoRequest::default(),
                    artifact_paths: ArtifactPathResolver::new(
                        TargetLayout::new(
                            PathBuf::from("_build"),
                            TargetLayoutMode::Workspace,
                            opt_level,
                            RunMode::Build,
                        ),
                        None,
                    ),
                    debug_export_build_plan: false,
                    enable_coverage: false,
                    moonc_output_json: false,
                    docs_serve: false,
                    warning_condition: WarningCondition::Default,
                    warn_list: None,
                };
                let plan = build_plan(
                    &resolved,
                    &config,
                    std::iter::empty(),
                    &InputDirective::default(),
                    None,
                    &UserLog::new(log::LevelFilter::Off),
                )
                .expect("empty plan should not need host tools");
                assert_eq!(
                    plan.backend_plan().direct_native_target(),
                    candidate.filter(|_| opt_level == OptLevel::Debug),
                    "candidate={candidate:?}, opt_level={opt_level:?}, collect_ref_cycle={collect_ref_cycle}"
                );
            }
        }
        let plain_exe = ArtifactKey::Executable {
            package: plain,
            target_kind: TargetKind::Source,
        };
        let flagged_exe = ArtifactKey::Executable {
            package: with_flags,
            target_kind: TargetKind::Source,
        };
        // Windows direct objects may constrain payload selection, but must not
        // replace a user-selected GCC-style driver with cl.exe.
        use moonutil::compiler_flags::{ARKind, CC, CCKind, Toolchain};
        for (kind, triple, permits_direct_object) in [
            (CCKind::Msvc, None, true),
            (CCKind::ClangCl, None, true),
            (CCKind::Clang, None, false),
            (CCKind::Gcc, None, false),
            (CCKind::ClangCl, Some("wasm32-unknown-none"), false),
        ] {
            let selected = Toolchain::from_env_override(CC {
                cc_kind: kind,
                cc_path: "selected-compiler".into(),
                ar_kind: ARKind::GnuAr,
                ar_path: "selected-archiver".into(),
                target_triple: triple.map(str::to_owned),
                is_env_override: true,
            });
            let mode = resolve_native_backend_mode(
                &resolved,
                std::slice::from_ref(&plain_exe),
                OptLevel::Debug,
                Some(NativeTarget::X86_64PcWindowsMsvc),
                [&selected],
            );
            assert_eq!(mode.direct_target().is_some(), permits_direct_object);
        }
        for (requested, permits_direct_object) in [
            (vec![], true),
            (vec![plain_exe.clone()], true),
            (
                vec![
                    plain_exe.clone(),
                    ArtifactKey::CoreIr {
                        package: with_flags,
                        target_kind: TargetKind::Source,
                    },
                ],
                true,
            ),
            (vec![flagged_exe.clone()], false),
            (vec![plain_exe, flagged_exe], false),
        ] {
            for candidate in [
                None,
                Some(NativeTarget::Aarch64AppleDarwin),
                Some(NativeTarget::X86_64UnknownLinuxGnu),
                Some(NativeTarget::X86_64PcWindowsMsvc),
            ] {
                let mode = resolve_native_backend_mode(
                    &resolved,
                    &requested,
                    OptLevel::Debug,
                    candidate,
                    std::iter::empty(),
                );
                assert_eq!(
                    mode.direct_target(),
                    if permits_direct_object {
                        candidate
                    } else {
                        None
                    },
                    "requested={requested:?}, candidate={candidate:?}"
                );
                let release_mode = resolve_native_backend_mode(
                    &resolved,
                    &requested,
                    OptLevel::Release,
                    candidate,
                    std::iter::empty(),
                );
                assert!(release_mode.direct_target().is_none());
            }
        }
    }

    #[test]
    fn project_packages_are_not_marked_as_dependency_artifacts() {
        assert_compiled_artifact_roles(false);
    }

    fn assert_compiled_artifact_roles(separate_module: bool) {
        let module_source = ModuleSource::local_path(
            "test/single"
                .parse::<ModuleName>()
                .expect("test module should parse"),
            PathBuf::from("."),
            DEFAULT_VERSION.clone(),
        )
        .expect("test module should have a valid version");
        let (mut modules, module) =
            ModuleDependencyGraph::only_one_module(module_source.clone(), moon_mod());
        let dependency_source = if separate_module {
            ModuleSource::local_path(
                "test/dependency"
                    .parse()
                    .expect("dependency module should parse"),
                PathBuf::from("../dependency"),
                DEFAULT_VERSION.clone(),
            )
            .expect("dependency module should have a valid version")
        } else {
            module_source.clone()
        };
        let mut dependency_info = moon_mod();
        dependency_info.name = dependency_source.name().to_string();
        let dependency_module =
            modules.add_module(dependency_source.clone(), dependency_info.clone().into());
        let mut packages = DiscoverResult::default();
        packages.test_register_module(module, moon_mod());
        packages.test_register_module(dependency_module, dependency_info);
        let script = packages.test_add_package(
            module,
            PackagePath::new("script").expect("script path should parse"),
            package(module, &module_source, "script", true, true),
        );
        let dependency = packages.test_add_package(
            dependency_module,
            PackagePath::new("dependency").expect("dependency path should parse"),
            package(
                dependency_module,
                &dependency_source,
                "dependency",
                false,
                false,
            ),
        );
        let script_target = script.build_target(TargetKind::Source);
        let dependency_target = dependency.build_target(TargetKind::Source);
        let mut relationship = PackageRelations::default();
        relationship.dep_graph.add_edge(
            script_target,
            dependency_target,
            DepEdge {
                short_alias: "dependency".into(),
                import_all: false,
                kind: TargetKind::Source,
            },
        );
        let mut module_dirs = DirSyncResult::default();
        module_dirs.insert(dependency_module, PathBuf::from("../dependency"));
        module_dirs.insert(module, PathBuf::from("."));
        let resolved = ResolvedProject {
            discovered: crate::resolve::DiscoveredProject {
                module_graph: modules,
                module_dirs,
                pkg_dirs: packages,
                enable_coverage: false,
            },
            package_relations: moonutil::target::TargetBackend::all()
                .iter()
                .map(|&b| (b, relationship.clone()))
                .collect(),
        };
        let artifact_paths = ArtifactPathResolver::new(
            TargetLayout::new(
                PathBuf::from("_build"),
                TargetLayoutMode::Mono {
                    main_module: module_source,
                },
                OptLevel::Debug,
                RunMode::Run,
            ),
            None,
        );
        let config = CompileConfig {
            backend: BackendConfig::WasmGc { use_wat: false },
            opt_level: OptLevel::Debug,
            action: RunMode::Run,
            debug_info: DebugInfoRequest::default(),
            artifact_paths,
            debug_export_build_plan: true,
            enable_coverage: false,
            moonc_output_json: false,
            docs_serve: false,
            warning_condition: WarningCondition::Default,
            warn_list: None,
        };

        let requested_artifacts = [ArtifactKey::Executable {
            package: script_target.package,
            target_kind: script_target.kind,
        }];
        let input_directive = InputDirective::default();
        let user_log = UserLog::new(log::LevelFilter::Error);
        let plan = build_plan(
            &resolved,
            &config,
            requested_artifacts.into_iter(),
            &input_directive,
            None,
            &user_log,
        )
        .expect("ordinary compile should plan one graph");
        let execution_plan = lower_build_plan(&resolved, &plan, &config, &[])
            .expect("ordinary compile should lower one graph");
        assert_eq!(execution_plan.action_ids().count(), 3);
        // The marker is already present before any scheduling policy reads it.
        for id in execution_plan.action_ids() {
            for path in execution_plan.action(id).outputs() {
                let declared = execution_plan
                    .declared_output(path)
                    .expect("output should be declared");
                let belongs_to_dependency =
                    declared.artifact().and_then(ArtifactKey::package) == Some(dependency);
                assert_eq!(
                    declared.is_dependency_artifact(),
                    separate_module && belongs_to_dependency
                );
            }
        }
        assert_eq!(plan.action_count(), 3);
        assert!(
            !plan
                .all_actions()
                .any(|action| action == BuildPlanNode::MakeExecutable(script_target).into())
        );
        assert_eq!(
            plan.artifact_provider(&ArtifactKey::Executable {
                package: script_target.package,
                target_kind: script_target.kind,
            }),
            BuildPlanNode::LinkCore(script_target).into()
        );
        assert!(
            plan.dependency_actions(&BuildPlanNode::BuildCore(script_target).into())
                .any(|action| action == BuildPlanNode::BuildCore(dependency_target).into())
        );

        assert_eq!(execution_plan.action_ids().count(), 3);
    }

    #[test]
    fn compile_prepares_proof_without_running_solver() {
        let module_source = ModuleSource::local_path(
            "test/single"
                .parse::<ModuleName>()
                .expect("test module should parse"),
            PathBuf::from("."),
            DEFAULT_VERSION.clone(),
        )
        .expect("test module should have a valid version");
        let (modules, module) =
            ModuleDependencyGraph::only_one_module(module_source.clone(), moon_mod());
        let mut packages = DiscoverResult::default();
        packages.test_register_module(module, moon_mod());
        let package = packages.test_add_package(
            module,
            PackagePath::new("proof").expect("proof path should parse"),
            package(module, &module_source, "proof", false, false),
        );
        let target = package.build_target(TargetKind::Source);
        let mut module_dirs = DirSyncResult::default();
        module_dirs.insert(module, PathBuf::from("."));
        let resolved = ResolvedProject {
            discovered: crate::resolve::DiscoveredProject {
                module_graph: modules,
                module_dirs,
                pkg_dirs: packages,
                enable_coverage: false,
            },
            package_relations: moonutil::target::TargetBackend::all()
                .iter()
                .map(|&b| (b, PackageRelations::default()))
                .collect(),
        };
        let artifact_paths = ArtifactPathResolver::new(
            TargetLayout::new(
                PathBuf::from("_build"),
                TargetLayoutMode::Mono {
                    main_module: module_source,
                },
                OptLevel::Debug,
                RunMode::Prove,
            ),
            None,
        );
        let config = CompileConfig {
            backend: BackendConfig::WasmGc { use_wat: false },
            opt_level: OptLevel::Debug,
            action: RunMode::Prove,
            debug_info: DebugInfoRequest::default(),
            artifact_paths,
            debug_export_build_plan: false,
            enable_coverage: false,
            moonc_output_json: false,
            docs_serve: false,
            warning_condition: WarningCondition::Default,
            warn_list: None,
        };

        let plan = build_plan(
            &resolved,
            &config,
            [ArtifactKey::ProofWhyml {
                package: target.package,
                target_kind: target.kind,
            }]
            .into_iter(),
            &InputDirective::default(),
            None,
            &UserLog::new(log::LevelFilter::Error),
        )
        .expect("proof preparation should plan");
        let execution_plan = lower_build_plan(&resolved, &plan, &config, &[])
            .expect("proof preparation should lower");

        assert!(execution_plan.action_ids().any(|id| {
            let args = execution_plan.action(id).command().args();
            args.iter().any(|arg| arg == "-emit-only")
                && !args
                    .iter()
                    .any(|arg| arg.starts_with("-why3") || arg == "-proof-report-output-path")
        }));
    }
}
