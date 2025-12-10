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

//! Loweing implementation for build nodes

use std::path::{Path, PathBuf};

use moonutil::{
    compiler_flags::{
        ArchiverConfigBuilder, CC, CCConfigBuilder, LinkerConfigBuilder, OptLevel as CCOptLevel,
        OutputType as CCOutputType, make_archiver_command, make_cc_command, make_cc_command_pure,
        make_linker_command_pure, resolve_cc,
    },
    cond_expr::OptLevel,
    mooncakes::{CORE_MODULE, ModuleId},
    package::JsFormat,
};
use petgraph::Direction;
use tracing::{Level, instrument};

use crate::{
    build_lower::{
        WarningCondition, artifact,
        compiler::{
            BuildCommonConfig, BuildCommonInput, CmdlineAbstraction, ErrorFormat, JsConfig,
            MiDependency, PackageSource, WasmConfig,
        },
    },
    build_plan::{BuildCStubsInfo, BuildTargetInfo, LinkCoreInfo, MakeExecutableInfo},
    discover::DiscoveredPackage,
    model::{BuildPlanNode, BuildTarget, PackageId, RunBackend, TargetKind},
    pkg_name::{PackageFQN, PackagePath},
    special_cases::{is_self_coverage_lib, should_skip_coverage},
};

use super::{BuildCommand, compiler, context::BuildPlanLowerContext};

impl<'a> BuildPlanLowerContext<'a> {
    fn is_module_third_party(&self, mid: ModuleId) -> bool {
        // This is usually a small vector, so this perf overhead is okay.
        !self.modules.input_module_ids().contains(&mid)
    }

    pub(super) fn set_flags(&self) -> compiler::CompilationFlags {
        compiler::CompilationFlags {
            no_opt: self.opt.opt_level == OptLevel::Debug,
            symbols: self.opt.debug_symbols,
            source_map: self.opt.target_backend.to_target().supports_source_map()
                && self.opt.debug_symbols,
            enable_coverage: false,
            self_coverage: false,
            enable_value_tracing: false,
        }
    }

    /// Returns `(enable_coverage, self_coverage)` for the given conditions
    ///
    /// - `target`: The build target, to determine if it's a blackbox test (bb
    ///   tests builds themselves don't need coverage)
    /// - `fqn`: The package FQN, to match against hardcoded exceptions
    /// - `is_build`: Whether this is a build command. BB tests need to be aware
    ///   of coverage but not apply it to themselves.
    pub(super) fn get_coverage_flags(
        &self,
        target: BuildTarget,
        fqn: &PackageFQN,
        is_build: bool,
    ) -> (bool, bool) {
        let enable_coverage = self.opt.enable_coverage
            && (!is_build || target.kind != TargetKind::BlackboxTest)
            && !should_skip_coverage(fqn);
        let self_coverage = enable_coverage && is_self_coverage_lib(fqn);
        (enable_coverage, self_coverage)
    }

    fn set_build_commons(
        &self,
        pkg: &DiscoveredPackage,
        info: &'a BuildTargetInfo,
        is_main: bool,
    ) -> BuildCommonConfig<'a> {
        // Standard library settings
        let stdlib_core_file = self
            .opt
            .stdlib_path
            .as_ref()
            .map(|x| artifact::core_bundle_path(x, self.opt.target_backend.into()).into());

        // Warning and error settings
        let error_format = if self.opt.moonc_output_json {
            ErrorFormat::Json
        } else {
            ErrorFormat::Regular
        };
        let deny_warn = self.opt.warning_condition == WarningCondition::Deny;
        let allow_warn = self.opt.warning_condition == WarningCondition::Allow;

        // Determine warn/alert config
        let (warn_config, alert_config) = if self.is_module_third_party(pkg.module) || allow_warn {
            // Third-party modules don't have any warnings or alerts
            (
                compiler::WarnAlertConfig::AllowAll,
                compiler::WarnAlertConfig::AllowAll,
            )
        } else {
            let wc = if let Some(w) = &info.warn_list {
                compiler::WarnAlertConfig::List(w.into())
            } else {
                compiler::WarnAlertConfig::default()
            };
            let ac = if let Some(a) = &info.alert_list {
                compiler::WarnAlertConfig::List(a.into())
            } else {
                compiler::WarnAlertConfig::default()
            };
            (wc, ac)
        };

        // Workspace settings
        let workspace_root = Some(
            self.module_dirs
                .get(pkg.module)
                .unwrap_or_else(|| {
                    panic!("Can't find module directory for {}, this is a bug", pkg.fqn)
                })
                .into(),
        );

        // Patch and MI/virtual config
        let patch_file = info.patch_file.as_deref().map(|x| x.into());

        // Compute -check-mi and virtual implementation mapping when requested
        let mut virtual_implementation = None;
        let mut check_mi = None;
        let mut no_mi = info.no_mi();

        if let Some(v_target) = info.check_mi_against {
            // The target to check against is always the Source target of the virtual package
            let mi_path = self.layout.mi_of_build_target(
                self.packages,
                &v_target,
                self.opt.target_backend.into(),
            );

            // If current package is NOT the same package as the virtual target,
            // this package is a concrete implementation → add -impl-virtual mapping.
            let v_pkg = self.packages.get_package(v_target.package);
            if v_pkg.fqn != pkg.fqn {
                virtual_implementation = Some(compiler::VirtualPackageImplementation {
                    mi_path: mi_path.into(),
                    package_name: &v_pkg.fqn,
                    package_path: v_pkg.root_path.as_path().into(),
                });
            } else {
                // Same package → this is a virtual package being checked against its own interface
                check_mi = Some(mi_path.into());
            }
            no_mi = true; // virtual implementations do not emit .mi because it cannot be imported directly
        }

        BuildCommonConfig {
            stdlib_core_file,
            error_format,
            deny_warn,
            warn_config,
            alert_config,
            patch_file,
            no_mi,
            workspace_root,
            is_main,

            check_mi,
            virtual_implementation,
            value_tracing: info.value_tracing,
        }
    }

    fn extend_extra_inputs(
        &self,
        commons: &BuildCommonConfig<'a>,
        extra_inputs: &mut Vec<PathBuf>,
    ) {
        // Also track any -check-mi file used by this command (virtual checks/impl)
        if let Some(p) = &commons.check_mi {
            extra_inputs.push(p.as_ref().to_path_buf());
        }
        if let Some(impl_v) = &commons.virtual_implementation {
            extra_inputs.push(impl_v.mi_path.as_ref().to_path_buf());
        }
        if let Some(patch) = &commons.patch_file {
            extra_inputs.push(patch.as_ref().to_path_buf());
        }
    }

    #[instrument(level = Level::DEBUG, skip(self, info))]
    pub(super) fn lower_check(
        &self,
        node: BuildPlanNode,
        target: BuildTarget,
        info: &BuildTargetInfo,
    ) -> BuildCommand {
        let package = self.get_package(target);
        let module = self.modules.module_info(package.module);

        let mi_output =
            self.layout
                .mi_of_build_target(self.packages, &target, self.opt.target_backend.into());
        let mi_inputs = self.mi_inputs_of(node, target);

        // Collect files iterator once so we can pass slices and extra inputs
        let files_vec = info.files().map(|x| x.to_owned()).collect::<Vec<_>>();

        // Determine whether the checked package is a main package.
        //
        // Black box tests does not include the source files of the original
        // package, while other kinds of package include those. Additionally,
        // no test drivers will be used in checking packages. Thus, black box
        // tests will definitely not contain a main function, while other
        // build targets will have the same kind of main function as the
        // original package.
        let is_main = match target.kind {
            TargetKind::BlackboxTest => false,
            TargetKind::Source
            | TargetKind::WhiteboxTest
            | TargetKind::InlineTest
            | TargetKind::SubPackage => package.raw.is_main,
        };
        let backend = self.opt.target_backend.into();
        let cmd = compiler::MooncCheck {
            required: BuildCommonInput::new(
                &files_vec,
                &info.doctest_files,
                &mi_inputs,
                compiler::CompiledPackageName::new(&package.fqn, target.kind),
                &package.root_path,
                self.layout.all_pkgs_of_build_target(backend),
                backend,
                target.kind,
            ),
            defaults: self.set_build_commons(package, info, is_main),
            mi_out: mi_output.into(),
            single_file: package.is_single_file,
            extra_flags: module.compile_flags.as_deref().unwrap_or_default(),
        };

        // Track doctest-only files as inputs as well
        let mut extra_inputs = files_vec.clone();
        extra_inputs.extend(info.doctest_files.clone());

        // Also track any -check-mi file used by this command (virtual checks/impl)
        self.extend_extra_inputs(&cmd.defaults, &mut extra_inputs);

        BuildCommand {
            extra_inputs,
            commandline: cmd.build_command(&*moonutil::BINARIES.moonc).into(),
        }
    }

    #[instrument(level = Level::DEBUG, skip(self, info))]
    pub(super) fn lower_build_mbt(
        &self,
        node: BuildPlanNode,
        target: BuildTarget,
        info: &BuildTargetInfo,
    ) -> BuildCommand {
        let package = self.get_package(target);
        let module = self.modules.module_info(package.module);

        let core_output = self.layout.core_of_build_target(
            self.packages,
            &target,
            self.opt.target_backend.into(),
        );
        let mi_output =
            self.layout
                .mi_of_build_target(self.packages, &target, self.opt.target_backend.into());

        let mi_inputs = self.mi_inputs_of(node, target);

        let mut files = info.files().map(|x| x.to_owned()).collect::<Vec<_>>();
        match target.kind {
            TargetKind::Source | TargetKind::SubPackage => {}
            TargetKind::WhiteboxTest | TargetKind::BlackboxTest | TargetKind::InlineTest => {
                files.push(self.layout.generated_test_driver(
                    self.packages,
                    &target,
                    self.opt.target_backend.into(),
                ));
            }
        };

        // Determine whether the built package is a main package.
        //
        // Different from checking, building test packages will always include
        // the test driver files, which will include the main function.
        let is_main = match target.kind {
            TargetKind::Source | TargetKind::SubPackage => package.raw.is_main,
            TargetKind::InlineTest | TargetKind::WhiteboxTest | TargetKind::BlackboxTest => true,
        };
        let backend = self.opt.target_backend.into();
        let mut cmd = compiler::MooncBuildPackage {
            required: BuildCommonInput::new(
                &files,
                &info.doctest_files,
                &mi_inputs,
                compiler::CompiledPackageName::new(&package.fqn, target.kind),
                &package.root_path,
                self.layout.all_pkgs_of_build_target(backend),
                backend,
                target.kind,
            ),
            defaults: self.set_build_commons(package, info, is_main),
            core_out: core_output.into(),
            mi_out: mi_output.into(),
            flags: self.set_flags(),
            extra_build_opts: module.compile_flags.as_deref().unwrap_or_default(),
        };
        // Propagate debug/coverage flags and common settings
        (cmd.flags.enable_coverage, cmd.flags.self_coverage) =
            self.get_coverage_flags(target, &package.fqn, true);
        cmd.defaults.no_mi |= target.kind.is_test();

        // Include doctest-only files as inputs to track dependency correctly
        // Note: This is the *extra* inputs, trivial dependencies are already
        // tracked via the build graph.
        let mut extra_inputs = info.files().map(|x| x.to_path_buf()).collect::<Vec<_>>();
        extra_inputs.extend(info.doctest_files.clone());

        self.extend_extra_inputs(&cmd.defaults, &mut extra_inputs);

        BuildCommand {
            commandline: cmd.build_command(&*moonutil::BINARIES.moonc).into(),
            extra_inputs,
        }
    }

    #[instrument(level = Level::DEBUG, skip(self, info))]
    pub(super) fn lower_link_core(
        &mut self,
        _node: BuildPlanNode,
        target: BuildTarget,
        info: &LinkCoreInfo,
    ) -> BuildCommand {
        let package = self.get_package(target);
        let module = self.modules.module_info(package.module);

        let mut core_input_files = Vec::new();
        // Add core for the standard library
        if let Some(stdlib) = &self.opt.stdlib_path {
            // The two stdlib core files must be linked in the correct order,
            // in order to get the correct order of initialization.
            if !info.abort_overridden {
                core_input_files.push(artifact::abort_core_path(
                    stdlib,
                    self.opt.target_backend.into(),
                ));
            }
            core_input_files.push(artifact::core_core_path(
                stdlib,
                self.opt.target_backend.into(),
            ));
        }
        // Linked core targets
        for target in &info.linked_order {
            let core_path = self.layout.core_of_build_target(
                self.packages,
                target,
                self.opt.target_backend.into(),
            );
            core_input_files.push(core_path);
        }

        let out_file = self.layout.linked_core_of_build_target(
            self.packages,
            &target,
            self.opt.target_backend.into(),
            self.opt.os,
            self.opt.output_wat,
        );

        let core_fqn = PackageFQN::new(CORE_MODULE.clone(), PackagePath::empty());
        let package_sources = info
            .linked_order
            .iter()
            .map(|target| {
                let pkg = self.packages.get_package(target.package);
                PackageSource {
                    package_name: compiler::CompiledPackageName::new(&pkg.fqn, target.kind),
                    source_dir: pkg.root_path.as_path().into(),
                }
            })
            .chain(self.opt.stdlib_path.as_ref().map(|p| PackageSource {
                package_name: compiler::CompiledPackageName::new(&core_fqn, TargetKind::Source),
                source_dir: p.into(),
            }))
            .collect::<Vec<_>>();

        let config_path = package.config_path();
        let cmd = compiler::MooncLinkCore {
            core_deps: &core_input_files,
            main_package: compiler::CompiledPackageName {
                fqn: &package.fqn,
                kind: target.kind,
            },
            output_path: out_file.into(),
            pkg_config_path: config_path.into(),
            package_sources: &package_sources,
            stdlib_core_source: None,
            target_backend: self.opt.target_backend.into(),
            flags: self.set_flags(),
            test_mode: target.kind.is_test(),
            wasm_config: self.get_wasm_config(package),
            js_config: self.get_js_config(target, package),
            exports: package.exported_functions(self.opt.target_backend.into()),
            extra_link_opts: module.link_flags.as_deref().unwrap_or_default(),
        };

        // Ensure n2 sees stdlib core bundle changes as inputs
        let mut extra_inputs = Vec::new();
        if let Some(stdlib) = &self.opt.stdlib_path {
            extra_inputs.push(artifact::abort_core_path(
                stdlib,
                self.opt.target_backend.into(),
            ));
            extra_inputs.push(artifact::core_core_path(
                stdlib,
                self.opt.target_backend.into(),
            ));
        }

        BuildCommand {
            extra_inputs,
            commandline: cmd.build_command(&*moonutil::BINARIES.moonc).into(),
        }
    }

    fn get_wasm_config<'b>(&self, pkg: &'b DiscoveredPackage) -> compiler::WasmConfig<'b> {
        let target = self.opt.target_backend;

        // WASM & WASM-GC has different config types, so we need to handle them separately
        if target == RunBackend::Wasm
            && let Some(cfg) = pkg.raw.link.as_ref().and_then(|x| x.wasm.as_ref())
        {
            WasmConfig {
                export_memory_name: cfg.export_memory_name.as_deref().map(|x| x.into()),
                import_memory: cfg.import_memory.as_ref(),
                memory_limits: cfg.memory_limits.as_ref(),
                shared_memory: cfg.shared_memory,
                heap_start_address: cfg.heap_start_address,
                link_flags: cfg.flags.as_deref(),
            }
        } else if target == RunBackend::WasmGC
            && let Some(cfg) = pkg.raw.link.as_ref().and_then(|x| x.wasm_gc.as_ref())
        {
            WasmConfig {
                export_memory_name: cfg.export_memory_name.as_deref().map(|x| x.into()),
                import_memory: cfg.import_memory.as_ref(),
                memory_limits: cfg.memory_limits.as_ref(),
                shared_memory: cfg.shared_memory,
                heap_start_address: None,
                link_flags: cfg.flags.as_deref(),
            }
        } else {
            WasmConfig::default()
        }
    }

    fn get_js_config(&self, target: BuildTarget, pkg: &DiscoveredPackage) -> Option<JsConfig> {
        let backend = self.opt.target_backend;
        if backend != RunBackend::Js {
            return None;
        }

        if target.kind.is_test() {
            return Some(JsConfig {
                format: Some(JsFormat::CJS),
                no_dts: true,
            });
        }

        // If link.js exists, use the specified, or default format.
        // Otherwise, omit.
        let format = pkg
            .raw
            .link
            .as_ref()
            .and_then(|x| x.js.as_ref())
            .map(|x| x.format.unwrap_or_default());

        Some(JsConfig {
            format,
            no_dts: false,
        })
    }

    #[instrument(level = Level::DEBUG, skip(self, info))]
    pub(super) fn lower_build_c_stub(
        &mut self,
        target: PackageId,
        index: u32,
        info: &BuildCStubsInfo,
    ) -> BuildCommand {
        assert!(
            self.opt.target_backend.is_native(),
            "Non-native make-executable should be already matched and should not be here"
        );

        let package = self.packages.get_package(target);

        let input_file = &package.c_stub_files[index as usize];
        let output_file = self.layout.c_stub_object_path(
            self.packages,
            target,
            input_file
                .file_stem()
                .expect("stub lib should have a file name"),
            self.opt.target_backend.into(),
            self.opt.os,
        );

        // Match legacy to_opt_level function exactly
        let opt_level = match (
            self.opt.opt_level == OptLevel::Release,
            self.opt.debug_symbols,
        ) {
            (true, false) => CCOptLevel::Speed,
            (true, true) => CCOptLevel::Debug,
            (false, true) => CCOptLevel::Debug,
            (false, false) => CCOptLevel::None,
        };
        // `tcc run` uses shared runtime, others use static runtime
        let use_shared_runtime = matches!(self.opt.target_backend, RunBackend::NativeTccRun);

        let config = CCConfigBuilder::default()
            .no_sys_header(true)
            .output_ty(CCOutputType::Object)
            .opt_level(opt_level)
            .debug_info(self.opt.debug_symbols)
            .link_moonbitrun(!use_shared_runtime)
            .define_use_shared_runtime_macro(use_shared_runtime)
            .build()
            .expect("Failed to build CC configuration for C stub");

        let intermediate_dir = self
            .layout
            .package_dir(&package.fqn, self.opt.target_backend.into())
            .display()
            .to_string();

        let cc_cmd = make_cc_command(
            self.opt.default_cc.clone(),
            info.stub_cc.clone(),
            config,
            &info.cc_flags,
            [input_file.display().to_string()],
            &intermediate_dir,
            &output_file.display().to_string(),
        );

        BuildCommand {
            commandline: cc_cmd.into(),
            extra_inputs: vec![input_file.clone()],
        }
    }

    #[instrument(level = Level::DEBUG, skip(self, info))]
    pub(super) fn lower_archive_or_link_c_stubs(
        &mut self,
        node: BuildPlanNode,
        target: PackageId,
        info: &BuildCStubsInfo,
    ) -> BuildCommand {
        assert!(
            self.opt.target_backend.is_native(),
            "Non-native make-executable should be already matched and should not be here"
        );

        let mut object_files = Vec::new();
        for (input, edge) in self.build_plan.dependency_edges(node) {
            // FIXME: we need a better way to separate the different kinds of input edges
            if !matches!(input, BuildPlanNode::BuildCStub { .. }) {
                continue;
            }
            self.append_artifact_of(input, edge, &mut object_files);
        }

        // There's two ways to handle this:
        // - When not using `tcc -run`, this creates an archive of the C stubs.
        // - When using `tcc -run`, this links the C stubs to an ELF .so, so tcc
        //   can load it at runtime.
        match self.opt.target_backend {
            RunBackend::WasmGC | RunBackend::Wasm | RunBackend::Js => {
                panic!("C stubs are not supported for non-native backends")
            }
            RunBackend::Native | RunBackend::Llvm => {
                self.lower_archive_c_stubs(target, info, &object_files)
            }
            RunBackend::NativeTccRun => self.lower_link_c_stubs(target, info, &object_files),
        }
    }

    fn lower_archive_c_stubs(
        &mut self,
        target: PackageId,
        info: &BuildCStubsInfo,
        object_files: &[PathBuf],
    ) -> BuildCommand {
        let archive = self.layout.c_stub_archive_path(
            self.packages,
            target,
            self.opt.target_backend.into(),
            self.opt.os,
        );

        let config = ArchiverConfigBuilder::default()
            .archive_moonbitrun(false)
            .build()
            .expect("Failed to build archiver configuration");

        let archiver_cmd = make_archiver_command(
            self.opt.default_cc.clone(),
            info.stub_cc.clone(), // TODO: no clone
            config,
            &object_files
                .iter()
                .map(|s| s.to_string_lossy())
                .collect::<Vec<_>>(),
            &archive.display().to_string(),
        );

        BuildCommand {
            extra_inputs: vec![],
            commandline: archiver_cmd.into(),
        }
    }

    fn lower_link_c_stubs(
        &mut self,
        target: PackageId,
        info: &BuildCStubsInfo,
        object_files: &[PathBuf],
    ) -> BuildCommand {
        // Output: lib{pkg}.{DYN_EXT}, exactly like legacy gen_link_stub_to_dynamic_lib_command()
        let dylib_out = self.layout.c_stub_link_dylib_path(
            self.packages,
            target,
            self.opt.target_backend.into(),
            self.opt.os,
        );
        let dest_dir = dylib_out
            .parent()
            .expect("c stub dylib should have a parent directory")
            .display()
            .to_string();

        // Track libruntime.{DYN_EXT} as a dependency but do not pass it as a direct linker src.
        // Legacy adds runtime into build inputs then links via -lruntime using link_shared_runtime.
        let runtime_dylib = self
            .layout
            .runtime_output_path(self.opt.target_backend, self.opt.os);
        let runtime_parent = runtime_dylib
            .parent()
            .expect("runtime dylib should have a parent directory");

        // Resolve CC: prefer stub_cc if provided, otherwise use default.
        let cc = resolve_cc(self.opt.default_cc.clone(), info.stub_cc.clone());

        // Build linker config: shared lib, no libmoonbitrun, and link shared runtime dir
        let lcfg = LinkerConfigBuilder::<&Path>::default()
            .link_moonbitrun(false) // this is only for tcc -run
            .output_ty(CCOutputType::SharedLib)
            .link_shared_runtime(Some(runtime_parent))
            .build()
            .expect("Failed to build LinkerConfig for C stub dylib");

        // Sources: only object files; runtime handled via link_shared_runtime (-lruntime + rpath)
        let sources: Vec<String> = object_files
            .iter()
            .map(|p| p.display().to_string())
            .collect();

        // User linker flags: stub_cc_link_flags (already parsed) from BuildCStubsInfo
        let link_flags: Vec<String> = info.link_flags.clone();

        let cc_cmd = make_linker_command_pure(
            cc,
            lcfg,
            &link_flags,
            &sources,
            &dest_dir,
            &dylib_out.display().to_string(),
            &self.opt.compiler_paths.lib_path,
        );

        // Note: Runtime input is tracked in build plan, so no need to add here.
        BuildCommand {
            extra_inputs: vec![],
            commandline: cc_cmd.into(),
        }
    }

    #[instrument(level = Level::DEBUG, skip(self, info))]
    pub(super) fn lower_make_exe(
        &mut self,
        target: BuildTarget,
        info: &MakeExecutableInfo,
    ) -> BuildCommand {
        match self.opt.target_backend {
            RunBackend::WasmGC | RunBackend::Wasm | RunBackend::Js => {
                panic!(
                    "Non-native make-executable should be already matched and should not be here"
                )
            }
            RunBackend::Native | RunBackend::Llvm => self.lower_build_exe_regular(target, info),
            RunBackend::NativeTccRun => self.build_tcc_run_driver_command(target, info),
        }
    }

    fn lower_build_exe_regular(
        &mut self,
        target: BuildTarget,
        info: &MakeExecutableInfo,
    ) -> BuildCommand {
        let _package = self.get_package(target);

        // Two things needs to be done here:
        // - compile the program (if needed)
        // - link with runtime library & artifacts of other C stubs
        // let cc_cmd = make_cc_command_pure(cc, config, user_cc_flags, src, dest_dir, dest, paths);

        let mut sources = vec![];
        // C artifact path
        self.append_all_artifacts_of(BuildPlanNode::LinkCore(target), &mut sources);
        // Runtime path
        self.append_all_artifacts_of(BuildPlanNode::BuildRuntimeLib, &mut sources);
        // C stubs to link
        for &stub_tgt in &info.link_c_stubs {
            self.append_all_artifacts_of(
                BuildPlanNode::ArchiveOrLinkCStubs(stub_tgt),
                &mut sources,
            );
        }

        let opt_level = match self.opt.opt_level {
            OptLevel::Release => CCOptLevel::Speed,
            OptLevel::Debug => CCOptLevel::Debug,
        };
        let config = CCConfigBuilder::default()
            .no_sys_header(true)
            .output_ty(CCOutputType::Executable) // TODO: support compiling to library
            .opt_level(opt_level)
            .debug_info(self.opt.debug_symbols)
            .link_moonbitrun(true)
            .define_use_shared_runtime_macro(false)
            .build()
            .expect("Failed to build CC configuration for executable");

        let dest = self
            .layout
            .executable_of_build_target(
                self.packages,
                &target,
                self.opt.target_backend,
                self.opt.os,
                true,
                self.opt.output_wat,
            )
            .display()
            .to_string();

        // This directory is used for MSVC to place intermediate files.
        // Each package should use their own to minimize conflicts.
        let pkg_dir = self
            .layout
            .package_dir(
                &self.get_package(target).fqn,
                self.opt.target_backend.into(),
            )
            .display()
            .to_string();

        let cc_cmd = make_cc_command_pure(
            resolve_cc(self.opt.default_cc.clone(), info.cc.clone()), // TODO: no clone
            config,
            &info.c_flags,
            sources.iter().map(|x| x.display().to_string()),
            &pkg_dir,
            Some(&dest),
            &self.opt.compiler_paths,
        );

        BuildCommand {
            extra_inputs: vec![],
            commandline: cc_cmd.into(),
        }
    }

    /// Build the command for `tcc -run` to execute when running, as well as
    /// putting that into a response file.
    fn build_tcc_run_driver_command(
        &self,
        target: BuildTarget,
        info: &MakeExecutableInfo,
    ) -> BuildCommand {
        // TODO: Get the CC instance from outside
        let cc = CC::internal_tcc().expect("Should not go to tcc run path without tcc available");

        let mut sources = vec![];

        // Runtime path
        self.append_all_artifacts_of(BuildPlanNode::BuildRuntimeLib, &mut sources);
        // C stubs to link
        for &stub_tgt in &info.link_c_stubs {
            self.append_all_artifacts_of(
                BuildPlanNode::ArchiveOrLinkCStubs(stub_tgt),
                &mut sources,
            );
        }

        let cfg = CCConfigBuilder::default()
            .no_sys_header(true) // -DMOONBIT_NATIVE_NO_SYS_HEADER for TCC
            .output_ty(CCOutputType::Executable) // base flags akin to "run"
            .opt_level(match self.opt.opt_level {
                OptLevel::Release => CCOptLevel::Speed,
                OptLevel::Debug => CCOptLevel::Debug,
            })
            .debug_info(self.opt.debug_symbols)
            .link_moonbitrun(false) // never link libmoonbitrun.o under tcc -run
            .define_use_shared_runtime_macro(true) // -DMOONBIT_USE_SHARED_RUNTIME (+ -fPIC on gcc-like)
            .build()
            .expect("Failed to build CC configuration for tcc-run");

        let mut cmdline = make_cc_command_pure(
            cc,
            cfg,
            &[] as &[&str], // no user flags
            sources.iter().map(|x| x.to_string_lossy().into_owned()),
            "", // TCC is not MSVC, no need to set special dest dir
            None,
            &self.opt.compiler_paths,
        );

        // The C file from moonc link-core
        cmdline.push("-run".to_string());
        let c_file = self.layout.linked_core_of_build_target(
            self.packages,
            &target,
            self.opt.target_backend.into(),
            self.opt.os,
            self.opt.output_wat,
        );
        cmdline.push(c_file.display().to_string());

        // Note: at this point, we have our TCC command.
        // However, this command should be executed when the user runs the final
        // executable, not in this build graph. Thus, we need to put them into
        // a response file so that `tcc` will run it later.
        //
        // We have a tool for this: `moon tool write-tcc-rsp-file <out> <args...>`
        let moonbuild = moonutil::BINARIES
            .moonbuild
            .to_str()
            .expect("moonbuild path is valid UTF-8");
        let mut rsp_cmdline = vec![
            moonbuild.to_string(),
            "tool".to_string(),
            "write-tcc-rsp-file".to_string(),
        ];
        let rsp_path = self.layout.executable_of_build_target(
            self.packages,
            &target,
            self.opt.target_backend,
            self.opt.os,
            false,
            self.opt.output_wat,
        );

        rsp_cmdline.push(rsp_path.display().to_string());
        rsp_cmdline.extend(cmdline.into_iter().skip(1)); // skip original `tcc` command

        BuildCommand {
            extra_inputs: vec![],
            commandline: rsp_cmdline.into(),
        }
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn lower_parse_mbti(&mut self, node: BuildPlanNode, pid: PackageId) -> BuildCommand {
        let pkg = self.packages.get_package(pid);
        let Some(mbti_path) = &pkg.virtual_mbti else {
            panic!(
                "Lowering ParseMbti node for non-virtual package {}, this is a bug",
                pkg.fqn
            );
        };

        // The virtual package interface is emitted as the `.mi` of the source target
        let target = pid.build_target(TargetKind::Source);
        let mi_out =
            self.layout
                .mi_of_build_target(self.packages, &target, self.opt.target_backend.into());

        // Resolve interface dependencies from the dep graph (path:alias pairs)
        let mi_inputs = self.mi_inputs_of(node, target);

        // Construct `moonc build-interface` command
        let mut cmd = compiler::MooncBuildInterface::new(
            mbti_path.as_path(),
            mi_out.as_path(),
            &mi_inputs,
            compiler::CompiledPackageName::new(&pkg.fqn, TargetKind::Source),
            &pkg.root_path,
        );

        // Provide std path when stdlib is enabled
        if let Some(stdlib_root) = &self.opt.stdlib_path {
            cmd.stdlib_core_file = Some(
                artifact::core_bundle_path(stdlib_root, self.opt.target_backend.into()).into(),
            );
        }

        BuildCommand {
            // Track the user-written `.mbti` contract as an explicit input
            extra_inputs: vec![mbti_path.clone()],
            commandline: cmd.build_command(&*moonutil::BINARIES.moonc).into(),
        }
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn mi_inputs_of(
        &self,
        _node: BuildPlanNode,
        target: BuildTarget,
    ) -> Vec<MiDependency<'a>> {
        let mut deps: Vec<MiDependency<'a>> = self
            .rel
            .dep_graph
            .edges_directed(target, Direction::Outgoing)
            // Skip `.mi` for standard library item `moonbitlang/core/abort`
            .filter(|(_, target, _)| {
                self.packages
                    .abort_pkg()
                    .is_none_or(|x| x != target.package)
            })
            .map(|(_, it, w)| {
                let in_file = self.layout.mi_of_build_target(
                    self.packages,
                    &it,
                    self.opt.target_backend.into(),
                );
                MiDependency::new(in_file, &w.short_alias)
            })
            .collect::<Vec<_>>();
        deps.sort_by(|x, y| x.alias.cmp(&y.alias));
        deps
    }
}
