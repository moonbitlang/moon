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
        make_archiver_command, make_cc_command, make_cc_command_pure, resolve_cc,
        ArchiverConfigBuilder, CCConfigBuilder, OptLevel as CCOptLevel, OutputType as CCOutputType,
        CC,
    },
    cond_expr::OptLevel,
    moon_dir::MOON_DIRS,
    mooncakes::{ModuleId, CORE_MODULE},
    package::JsFormat,
};
use petgraph::Direction;
use tracing::{instrument, Level};

use crate::{
    build_lower::{
        artifact,
        compiler::{
            BuildCommonArgs, CmdlineAbstraction, ErrorFormat, MiDependency, PackageSource,
            WasmConfig,
        },
    },
    build_plan::{BuildCStubsInfo, BuildTargetInfo, LinkCoreInfo, MakeExecutableInfo},
    discover::DiscoveredPackage,
    model::{BuildPlanNode, BuildTarget, PackageId, TargetKind},
    pkg_name::{PackageFQN, PackagePath},
};

use super::{compiler, context::BuildPlanLowerContext, BuildCommand};

impl<'a> BuildPlanLowerContext<'a> {
    fn is_module_third_party(&self, mid: ModuleId) -> bool {
        // This is usually a small vector, so this perf overhead is okay.
        !self.modules.input_module_ids().contains(&mid)
    }

    pub(super) fn set_flags(&self) -> compiler::CompilationFlags {
        compiler::CompilationFlags {
            no_opt: self.opt.opt_level == OptLevel::Debug,
            symbols: self.opt.debug_symbols,
            source_map: matches!(
                self.opt.target_backend,
                TargetBackend::Js | TargetBackend::WasmGC
            ) && self.opt.debug_symbols,
            enable_coverage: false,
            self_coverage: false,
            enable_value_tracing: false,
        }
    }

    fn set_build_commons(
        &self,
        common: &mut BuildCommonArgs<'a>,
        pkg: &DiscoveredPackage,
        info: &'a BuildTargetInfo,
    ) {
        // Standard library settings
        common.stdlib_core_file = self
            .opt
            .stdlib_path
            .as_ref()
            .map(|x| artifact::core_bundle_path(x, self.opt.target_backend).into());

        // Warning and error settings
        common.error_format = if self.opt.moonc_output_json {
            ErrorFormat::Json
        } else {
            ErrorFormat::Regular
        };
        common.deny_warn = self.opt.deny_warn;

        if self.is_module_third_party(pkg.module) {
            // Third-party modules don't have any warnings or alerts
            common.warn_config = compiler::WarnAlertConfig::AllowAll;
            common.alert_config = compiler::WarnAlertConfig::AllowAll;
        } else {
            if let Some(w) = &info.warn_list {
                common.warn_config = compiler::WarnAlertConfig::List(w.into());
            }
            if let Some(a) = &info.alert_list {
                common.alert_config = compiler::WarnAlertConfig::List(a.into());
            }
        }

        // Workspace settings
        common.workspace_root = Some(
            self.module_dirs
                .get(pkg.module)
                .unwrap_or_else(|| {
                    panic!("Can't find module directory for {}, this is a bug", pkg.fqn)
                })
                .into(),
        );
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

        let mut cmd = compiler::MooncCheck {
            common: compiler::BuildCommonArgs::new(
                &files_vec,
                &mi_inputs,
                compiler::CompiledPackageName::new(&package.fqn, target.kind),
                &package.root_path,
                self.opt.target_backend,
                target.kind,
            ),
            mi_out: mi_output.into(),
            no_mi: false,
            is_third_party: false,
            single_file: false,
            patch_file: None,
        };
        // Wire doctest-only files to common so they are passed as `-doctest-only <file>`
        cmd.common.doctest_only_sources = &info.doctest_files;
        self.set_build_commons(&mut cmd.common, package, info);

        // Determine whether the checked package is a main package.
        //
        // Black box tests does not include the source files of the original
        // package, while other kinds of package include those. Additionally,
        // no test drivers will be used in checking packages. Thus, black box
        // tests will definitely not contain a main function, while other
        // build targets will have the same kind of main function as the
        // original package.
        cmd.common.is_main = match target.kind {
            TargetKind::BlackboxTest => false,
            TargetKind::Source
            | TargetKind::WhiteboxTest
            | TargetKind::InlineTest
            | TargetKind::SubPackage => package.raw.is_main,
        };

        // Track doctest-only files as inputs as well
        let mut extra_inputs = files_vec.clone();
        extra_inputs.extend(info.doctest_files.clone());
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

        let mut cmd = compiler::MooncBuildPackage {
            common: compiler::BuildCommonArgs::new(
                &files,
                &mi_inputs,
                compiler::CompiledPackageName::new(&package.fqn, target.kind),
                &package.root_path,
                self.opt.target_backend,
                target.kind,
            ),
            core_out: core_output.into(),
            mi_out: mi_output.into(),
            no_mi: false,
            flags: self.set_flags(),
            extra_build_opts: &[],
        };
        // Propagate debug/coverage flags and common settings
        cmd.common.doctest_only_sources = &info.doctest_files;
        cmd.flags.no_opt = self.opt.opt_level == OptLevel::Debug;
        cmd.flags.symbols = self.opt.debug_symbols;
        cmd.flags.enable_coverage = self.opt.enable_coverage;
        self.set_build_commons(&mut cmd.common, package, info);

        // Determine whether the built package is a main package.
        //
        // Different from checking, building test packages will always include
        // the test driver files, which will include the main function.
        cmd.common.is_main = match target.kind {
            TargetKind::Source | TargetKind::SubPackage => package.raw.is_main,
            TargetKind::InlineTest | TargetKind::WhiteboxTest | TargetKind::BlackboxTest => true,
        };

        // TODO: a lot of knobs are not controlled here

        // Include doctest-only files as inputs to track dependency correctly
        let mut extra_inputs = files.clone();
        extra_inputs.extend(info.doctest_files.clone());

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
            core_input_files.push(artifact::abort_core_path(stdlib, self.opt.target_backend));
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
        let mut cmd = compiler::MooncLinkCore {
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
            js_format: self.get_js_format(package),
            extra_link_opts: &[],
        };

        // JS format settings
        if self.opt.target_backend == TargetBackend::Js {
            if package.raw.force_link || package.raw.is_main {
                cmd.js_format = Some(JsFormat::default());
            } else if let Some(link) = package.raw.link.as_ref().and_then(|x| x.js.as_ref()) {
                cmd.js_format = Some(link.format.unwrap_or_default());
            }
        }

        BuildCommand {
            extra_inputs: vec![],
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
            exports: cfg.exports.as_deref(),
            export_memory_name: cfg.export_memory_name.as_deref().map(|x| x.into()),
            import_memory: cfg.import_memory.as_ref(),
            memory_limits: cfg.memory_limits.as_ref(),
            shared_memory: cfg.shared_memory,
            heap_start_address: cfg.heap_start_address,
            link_flags: cfg.flags.as_deref(),
        }
    }

    fn get_js_format(&self, pkg: &DiscoveredPackage) -> Option<JsFormat> {
        let target = self.opt.target_backend;
        if target != TargetBackend::Js {
            return None;
        }

        let linking_config = pkg.raw.link.as_ref().and_then(|x| x.js.as_ref());
        if let Some(cfg) = linking_config {
            cfg.format
        } else {
            Some(JsFormat::ESM)
        }
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
                .file_name()
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
        for input in self.build_plan.dependency_nodes(node) {
            self.append_artifact_of(input, &mut object_files);
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
        self.append_artifact_of(BuildPlanNode::LinkCore(target), &mut sources);
        // Runtime path
        self.append_artifact_of(BuildPlanNode::BuildRuntimeLib, &mut sources);
        // C stubs to link
        for &stub_tgt in &info.link_c_stubs {
            self.append_artifact_of(BuildPlanNode::ArchiveCStubs(stub_tgt.package), &mut sources);
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
    pub(super) fn mi_inputs_of(
        &self,
        _node: BuildPlanNode,
        target: BuildTarget,
    ) -> Vec<MiDependency<'_>> {
        self.rel
            .dep_graph
            .edges_directed(target, Direction::Outgoing)
            .map(|(_, it, w)| {
                let in_file =
                    self.layout
                        .mi_of_build_target(self.packages, &it, self.opt.target_backend);
                MiDependency::new(in_file, &w.short_alias)
            })
            .collect::<Vec<_>>()
    }
}
