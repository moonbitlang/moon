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

//! Specific lowering implementations for different build node types.

use moonutil::{
    common::{DriverKind, TargetBackend},
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

use crate::{
    build_lower::{
        artifact,
        compiler::{CmdlineAbstraction, MiDependency, Mooninfo, PackageSource},
    },
    build_plan::{BuildCStubsInfo, BuildTargetInfo, LinkCoreInfo, MakeExecutableInfo},
    model::{BuildPlanNode, BuildTarget, PackageId, TargetKind},
    pkg_name::{PackageFQN, PackagePath},
};

use super::{compiler, context::BuildPlanLowerContext, BuildCommand};

impl<'a> BuildPlanLowerContext<'a> {
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

        let mut cmd = compiler::MooncCheck::new(
            &files_vec,
            &mi_output,
            &mi_inputs,
            compiler::CompiledPackageName::new(&package.fqn, target.kind),
            &package.root_path,
            self.opt.target_backend,
            target.kind,
        );
        self.set_commons(&mut cmd.common);

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

        BuildCommand {
            extra_inputs: files_vec.clone(),
            commandline: cmd.build_command("moonc"),
        }
    }

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

        let mut cmd = compiler::MooncBuildPackage::new(
            &files,
            &core_output,
            &mi_output,
            &mi_inputs,
            compiler::CompiledPackageName::new(&package.fqn, target.kind),
            &package.root_path,
            self.opt.target_backend,
            target.kind,
        );
        cmd.flags.no_opt = self.opt.opt_level == OptLevel::Debug;
        cmd.flags.symbols = self.opt.debug_symbols;
        self.set_commons(&mut cmd.common);

        // Determine whether the built package is a main package.
        //
        // Different from checking, building test packages will always include
        // the test driver files, which will include the main function.
        cmd.common.is_main = match target.kind {
            TargetKind::Source | TargetKind::SubPackage => package.raw.is_main,
            TargetKind::InlineTest | TargetKind::WhiteboxTest | TargetKind::BlackboxTest => true,
        };

        // TODO: a lot of knobs are not controlled here

        BuildCommand {
            commandline: cmd.build_command("moonc"),
            extra_inputs: files,
        }
    }

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
        let mut cmd = compiler::MooncLinkCore::new(
            &core_input_files,
            compiler::CompiledPackageName {
                fqn: &package.fqn,
                kind: target.kind,
            },
            &out_file,
            &config_path,
            &package_sources,
            self.opt.target_backend,
            target.kind.is_test(),
        );
        self.set_flags(&mut cmd.flags);

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

    pub(super) fn lower_gen_test_driver(
        &mut self,
        _node: BuildPlanNode,
        target: BuildTarget,
        info: &BuildTargetInfo,
    ) -> BuildCommand {
        let package = self.get_package(target);
        let output_driver =
            self.layout
                .generated_test_driver(self.packages, &target, self.opt.target_backend);
        let output_metadata = self.layout.generated_test_driver_metadata(
            self.packages,
            &target,
            self.opt.target_backend,
        );
        let driver_kind = match target.kind {
            TargetKind::Source => panic!("Source package cannot be a test driver"),
            TargetKind::WhiteboxTest => DriverKind::Whitebox,
            TargetKind::BlackboxTest => DriverKind::Blackbox,
            TargetKind::InlineTest => DriverKind::Internal,
            TargetKind::SubPackage => panic!("Sub-package cannot be a test driver"),
        };
        let pkg_full_name = package.fqn.to_string();
        let files_vec = info.files().map(|x| x.to_owned()).collect::<Vec<_>>();

        let cmd = compiler::MoonGenTestDriver::new(
            &files_vec,
            output_driver,
            output_metadata,
            self.opt.target_backend,
            &pkg_full_name,
            driver_kind,
        );

        BuildCommand {
            commandline: cmd.build_command("moon"),
            extra_inputs: files_vec,
        }
    }

    pub(super) fn lower_bundle(
        &mut self,
        node: BuildPlanNode,
        module_id: ModuleId,
    ) -> BuildCommand {
        let module = self.modules.mod_name_from_id(module_id);
        let output = self
            .layout
            .bundle_result_path(self.opt.target_backend, module.name());

        let mut inputs = vec![];
        for dep in self.build_plan.dependency_nodes(node) {
            let BuildPlanNode::BuildCore(package) = dep else {
                panic!("Bundle node can only depend on BuildCore nodes");
            };
            inputs.push(self.layout.core_of_build_target(
                self.packages,
                &package,
                self.opt.target_backend,
            ));
        }

        let cmd = compiler::MooncBundleCore::new(&inputs, output);

        BuildCommand {
            extra_inputs: vec![],
            commandline: cmd.build_command("moonc"),
        }
    }

    pub(super) fn lower_compile_runtime(&mut self) -> BuildCommand {
        let artifact_path = self
            .layout
            .runtime_output_path(self.opt.target_backend, self.opt.os);

        // TODO: this part might need more simplification?
        let runtime_c_path = self.opt.runtime_dot_c_path.clone();
        let cc_cmd = make_cc_command_pure::<&'static str>(
            resolve_cc(CC::default(), None),
            CCConfigBuilder::default()
                .no_sys_header(true)
                .output_ty(CCOutputType::Object)
                .opt_level(CCOptLevel::Speed)
                .debug_info(true)
                // always link moonbitrun in this mode
                .link_moonbitrun(true)
                .define_use_shared_runtime_macro(false)
                .build()
                .expect("Failed to build CC configuration for runtime"),
            &[],
            [runtime_c_path.display().to_string()],
            &self.opt.target_dir_root.display().to_string(),
            &artifact_path.display().to_string(),
            &self.opt.compiler_paths,
        );

        BuildCommand {
            extra_inputs: vec![runtime_c_path],
            commandline: cc_cmd,
        }
    }

    pub(super) fn lower_generate_mbti(&mut self, target: BuildTarget) -> BuildCommand {
        let input = self
            .layout
            .mi_of_build_target(self.packages, &target, self.opt.target_backend);
        let pkg = self.packages.get_package(target.package);
        let output = self.layout.generated_mbti_path(&pkg.root_path);

        let cmd = Mooninfo {
            mi_in: input.into(),
            out: output.into(),
            no_alias: false, // TODO: fill this
        };

        BuildCommand {
            extra_inputs: vec![],
            commandline: cmd.build_command("mooninfo"),
        }
    }

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
