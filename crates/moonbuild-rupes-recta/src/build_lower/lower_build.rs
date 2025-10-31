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

use moonutil::{
    common::TargetBackend,
    compiler_flags::{
        ArchiverConfigBuilder, CC, CCConfigBuilder, OptLevel as CCOptLevel,
        OutputType as CCOutputType, make_archiver_command, make_cc_command, make_cc_command_pure,
        resolve_cc,
    },
    cond_expr::OptLevel,
    moon_dir::MOON_DIRS,
    mooncakes::{CORE_MODULE, ModuleId},
    package::JsFormat,
};
use petgraph::Direction;
use tracing::{Level, instrument};

use crate::{
    build_lower::{
        artifact,
        compiler::{
            BuildCommonConfig, BuildCommonInput, CmdlineAbstraction, ErrorFormat, JsConfig,
            MiDependency, PackageSource, WasmConfig,
        },
    },
    build_plan::{BuildCStubsInfo, BuildTargetInfo, LinkCoreInfo, MakeExecutableInfo},
    discover::DiscoveredPackage,
    model::{BuildPlanNode, BuildTarget, PackageId, TargetKind},
    pkg_name::{PackageFQN, PackagePath},
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
            source_map: self.opt.target_backend.supports_source_map() && self.opt.debug_symbols,
            enable_coverage: false,
            self_coverage: false,
            enable_value_tracing: false,
        }
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
            .map(|x| artifact::core_bundle_path(x, self.opt.target_backend).into());

        // Warning and error settings
        let error_format = if self.opt.moonc_output_json {
            ErrorFormat::Json
        } else {
            ErrorFormat::Regular
        };
        let deny_warn = self.opt.deny_warn;

        // Determine warn/alert config
        let (warn_config, alert_config) = if self.is_module_third_party(pkg.module) {
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

        if let Some(v_target) = info.check_mi_against {
            // The target to check against is always the Source target of the virtual package
            let mi_path =
                self.layout
                    .mi_of_build_target(self.packages, &v_target, self.opt.target_backend);

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
        }

        let no_mi = info.no_mi();

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
        let mi_output =
            self.layout
                .mi_of_build_target(self.packages, &target, self.opt.target_backend);
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

        let cmd = compiler::MooncCheck {
            required: BuildCommonInput::new(
                &files_vec,
                &info.doctest_files,
                &mi_inputs,
                compiler::CompiledPackageName::new(&package.fqn, target.kind),
                &package.root_path,
                self.opt.target_backend,
                target.kind,
            ),
            defaults: self.set_build_commons(package, info, is_main),
            mi_out: mi_output.into(),
            single_file: false,
        };

        // Track doctest-only files as inputs as well
        let mut extra_inputs = files_vec.clone();
        extra_inputs.extend(info.doctest_files.clone());

        // Also track any -check-mi file used by this command (virtual checks/impl)
        if let Some(p) = &cmd.defaults.check_mi {
            extra_inputs.push(p.as_ref().to_path_buf());
        }
        if let Some(impl_v) = &cmd.defaults.virtual_implementation {
            extra_inputs.push(impl_v.mi_path.as_ref().to_path_buf());
        }

        BuildCommand {
            extra_inputs,
            commandline: cmd.build_command("moonc"),
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
        let core_output =
            self.layout
                .core_of_build_target(self.packages, &target, self.opt.target_backend);
        let mi_output =
            self.layout
                .mi_of_build_target(self.packages, &target, self.opt.target_backend);

        let mi_inputs = self.mi_inputs_of(node, target);

        let mut files = info.files().map(|x| x.to_owned()).collect::<Vec<_>>();
        match target.kind {
            TargetKind::Source | TargetKind::SubPackage => {}
            TargetKind::WhiteboxTest | TargetKind::BlackboxTest | TargetKind::InlineTest => {
                files.push(self.layout.generated_test_driver(
                    self.packages,
                    &target,
                    self.opt.target_backend,
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

        let mut cmd = compiler::MooncBuildPackage {
            required: BuildCommonInput::new(
                &files,
                &info.doctest_files,
                &mi_inputs,
                compiler::CompiledPackageName::new(&package.fqn, target.kind),
                &package.root_path,
                self.opt.target_backend,
                target.kind,
            ),
            defaults: self.set_build_commons(package, info, is_main),
            core_out: core_output.into(),
            mi_out: mi_output.into(),
            flags: self.set_flags(),
            extra_build_opts: &[],
        };
        // Propagate debug/coverage flags and common settings
        cmd.flags.enable_coverage = self.opt.enable_coverage;
        cmd.defaults.no_mi |= target.kind.is_test();

        // TODO: a lot of knobs are not controlled here

        // Include doctest-only files as inputs to track dependency correctly
        let mut extra_inputs = files.clone();
        extra_inputs.extend(info.doctest_files.clone());

        // Also track any -check-mi file used by this command (virtual checks/impl)
        if let Some(p) = &cmd.defaults.check_mi {
            extra_inputs.push(p.as_ref().to_path_buf());
        }
        if let Some(impl_v) = &cmd.defaults.virtual_implementation {
            extra_inputs.push(impl_v.mi_path.as_ref().to_path_buf());
        }

        BuildCommand {
            commandline: cmd.build_command("moonc"),
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
        let mut core_input_files = Vec::new();
        // Add core for the standard library
        if let Some(stdlib) = &self.opt.stdlib_path {
            // The two stdlib core files must be linked in the correct order,
            // in order to get the correct order of initialization.
            if !info.abort_overridden {
                core_input_files.push(artifact::abort_core_path(stdlib, self.opt.target_backend));
            }
            core_input_files.push(artifact::core_core_path(stdlib, self.opt.target_backend));
        }
        // Linked core targets
        for target in &info.linked_order {
            let core_path =
                self.layout
                    .core_of_build_target(self.packages, target, self.opt.target_backend);
            core_input_files.push(core_path);
        }

        let out_file = self.layout.linked_core_of_build_target(
            self.packages,
            &target,
            self.opt.target_backend,
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
            target_backend: self.opt.target_backend,
            flags: self.set_flags(),
            test_mode: target.kind.is_test(),
            wasm_config: self.get_wasm_config(package),
            js_config: self.get_js_config(target, package),
            exports: package.exported_functions(self.opt.target_backend),
            extra_link_opts: &[],
        };

        // Ensure n2 sees stdlib core bundle changes as inputs
        let mut extra_inputs = Vec::new();
        if let Some(stdlib) = &self.opt.stdlib_path {
            extra_inputs.push(artifact::abort_core_path(stdlib, self.opt.target_backend));
            extra_inputs.push(artifact::core_core_path(stdlib, self.opt.target_backend));
        }

        BuildCommand {
            extra_inputs,
            commandline: cmd.build_command("moonc"),
        }
    }

    fn get_wasm_config<'b>(&self, pkg: &'b DiscoveredPackage) -> compiler::WasmConfig<'b> {
        let target = self.opt.target_backend;
        if target != TargetBackend::Wasm {
            return WasmConfig::default();
        }

        let linking_config = pkg.raw.link.as_ref().and_then(|x| x.wasm.as_ref());
        let Some(cfg) = linking_config else {
            return WasmConfig::default();
        };

        WasmConfig {
            export_memory_name: cfg.export_memory_name.as_deref().map(|x| x.into()),
            import_memory: cfg.import_memory.as_ref(),
            memory_limits: cfg.memory_limits.as_ref(),
            shared_memory: cfg.shared_memory,
            heap_start_address: cfg.heap_start_address,
            link_flags: cfg.flags.as_deref(),
        }
    }

    fn get_js_config(&self, target: BuildTarget, pkg: &DiscoveredPackage) -> Option<JsConfig> {
        let backend = self.opt.target_backend;
        if backend != TargetBackend::Js {
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
            self.opt.target_backend,
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

        let config = CCConfigBuilder::default()
            .no_sys_header(true)
            .output_ty(CCOutputType::Object)
            .opt_level(opt_level)
            .debug_info(self.opt.debug_symbols)
            .link_moonbitrun(true) // TODO: support use_tcc_run flag when available
            .define_use_shared_runtime_macro(false) // TODO: support use_tcc_run flag when available
            .build()
            .expect("Failed to build CC configuration for C stub");

        let cc_cmd = make_cc_command(
            CC::default(),
            info.stub_cc.clone(),
            config,
            &info.cc_flags,
            [input_file.display().to_string()],
            &MOON_DIRS.moon_lib_path.display().to_string(),
            &output_file.display().to_string(),
        );

        BuildCommand {
            commandline: cc_cmd,
            extra_inputs: vec![input_file.clone()],
        }
    }

    #[instrument(level = Level::DEBUG, skip(self, info))]
    pub(super) fn lower_archive_c_stubs(
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
            self.append_artifact_of(input, edge, &mut object_files);
        }

        // Match legacy: create archive name as lib{pkgname}.{A_EXT}
        let archive = self.layout.c_stub_archive_path(
            self.packages,
            target,
            self.opt.target_backend,
            self.opt.os,
        );

        let config = ArchiverConfigBuilder::default()
            .archive_moonbitrun(false)
            .build()
            .expect("Failed to build archiver configuration");

        let archiver_cmd = make_archiver_command(
            CC::default(),
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
            commandline: archiver_cmd,
        }
    }

    #[instrument(level = Level::DEBUG, skip(self, info))]
    pub(super) fn lower_make_exe(
        &mut self,
        target: BuildTarget,
        info: &MakeExecutableInfo,
    ) -> BuildCommand {
        assert!(
            self.opt.target_backend.is_native(),
            "Non-native make-executable should be already matched and should not be here"
        );

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
                BuildPlanNode::ArchiveCStubs(stub_tgt.package),
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
            .debug_info(self.opt.opt_level == OptLevel::Debug)
            .link_moonbitrun(true) // TODO: support `tcc run`
            .define_use_shared_runtime_macro(false)
            .build()
            .expect("Failed to build CC configuration for executable");
        let cc_cmd = make_cc_command_pure(
            resolve_cc(CC::default(), info.cc.clone()), // TODO: no clone
            config,
            &info.c_flags,
            sources.iter().map(|x| x.display().to_string()),
            &self.opt.target_dir_root.display().to_string(),
            &self
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
                .to_string(),
            &self.opt.compiler_paths,
        );

        BuildCommand {
            extra_inputs: vec![],
            commandline: cc_cmd,
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
                .mi_of_build_target(self.packages, &target, self.opt.target_backend);

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
            cmd.stdlib_core_file =
                Some(artifact::core_bundle_path(stdlib_root, self.opt.target_backend).into());
        }

        BuildCommand {
            // Track the user-written `.mbti` contract as an explicit input
            extra_inputs: vec![mbti_path.clone()],
            commandline: cmd.build_command("moonc"),
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
                let in_file =
                    self.layout
                        .mi_of_build_target(self.packages, &it, self.opt.target_backend);
                MiDependency::new(in_file, &w.short_alias)
            })
            .collect::<Vec<_>>();
        deps.sort_by(|x, y| x.path.cmp(&y.path));
        deps
    }
}
