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

//! Lowering implementation for auxiliary build steps.

use moonutil::{
    common::DriverKind,
    compiler_flags::{
        CC, CCConfigBuilder, OptLevel as CCOptLevel, OutputType as CCOutputType,
        make_cc_command_pure, resolve_cc,
    },
    mooncakes::{ModuleId, ModuleSourceKind},
};
use tracing::{Level, instrument};

use crate::{
    build_lower::{
        Commandline,
        compiler::{CmdlineAbstraction, MoondocCommand, Mooninfo},
    },
    build_plan::BuildTargetInfo,
    model::{BuildPlanNode, BuildTarget, PackageId, RunBackend, TargetKind},
};

use super::{BuildCommand, compiler};

impl<'a> super::BuildPlanLowerContext<'a> {
    #[instrument(level = Level::DEBUG, skip(self, info))]
    pub(super) fn lower_gen_test_driver(
        &mut self,
        _node: BuildPlanNode,
        target: BuildTarget,
        info: &BuildTargetInfo,
    ) -> BuildCommand {
        let package = self.get_package(target);
        let output_driver = self.layout.generated_test_driver(
            self.packages,
            &target,
            self.opt.target_backend.into(),
        );
        let output_metadata = self.layout.generated_test_driver_metadata(
            self.packages,
            &target,
            self.opt.target_backend.into(),
        );
        let driver_kind = match target.kind {
            TargetKind::Source => panic!("Source package cannot be a test driver"),
            TargetKind::WhiteboxTest => DriverKind::Whitebox,
            TargetKind::BlackboxTest => DriverKind::Blackbox,
            TargetKind::InlineTest => DriverKind::Internal,
            TargetKind::SubPackage => panic!("Sub-package cannot be a test driver"),
        };
        let pkg_full_name = package.fqn.to_string();
        let files_vec = if target.kind == TargetKind::WhiteboxTest {
            info.whitebox_files.clone()
        } else {
            info.files().map(|x| x.to_owned()).collect::<Vec<_>>()
        };
        let patch_file = info.patch_file.as_deref().map(|x| x.into());

        let (enable_coverage, self_coverage) =
            self.get_coverage_flags(target, package, &package.fqn, false);

        let cmd = compiler::MoonGenTestDriver {
            files: &files_vec,
            doctest_only_files: &info.doctest_files,
            output_driver: output_driver.into(),
            output_metadata: output_metadata.into(),
            bench: self.opt.action == moonutil::common::RunMode::Bench,
            enable_coverage,
            coverage_package_override: if self_coverage { Some("@self") } else { None },
            driver_kind,
            target_backend: self.opt.target_backend.into(),
            patch_file,
            pkg_name: &pkg_full_name,
            max_concurrent_tests: package.raw.max_concurrent_tests,
        };

        let commandline = cmd.build_command(&*moonutil::BINARIES.moonbuild);

        // Track doctest files as extra inputs
        let mut extra_inputs = files_vec;
        extra_inputs.extend_from_slice(&info.doctest_files);

        BuildCommand {
            commandline: commandline.into(),
            extra_inputs,
        }
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn lower_bundle(
        &mut self,
        node: BuildPlanNode,
        module_id: ModuleId,
    ) -> BuildCommand {
        let module = self.modules.mod_name_from_id(module_id);
        let output = self
            .layout
            .bundle_result_path(self.opt.target_backend.into(), module.name());
        let info = self
            .build_plan
            .bundle_info(module_id)
            .expect("Bundle info should be present when lowering bundle node");

        let mut inputs = vec![];
        for dep in info.bundle_targets.iter() {
            inputs.push(self.layout.core_of_build_target(
                self.packages,
                dep,
                self.opt.target_backend.into(),
            ));
        }

        let cmd = compiler::MooncBundleCore::new(&inputs, output);

        BuildCommand {
            extra_inputs: vec![],
            commandline: cmd.build_command(&*moonutil::BINARIES.moonc).into(),
        }
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn lower_compile_runtime(&mut self) -> BuildCommand {
        let artifact_path = self
            .layout
            .runtime_output_path(self.opt.target_backend, self.opt.os);

        let runtime_c_path = self.opt.runtime_dot_c_path.clone();

        let output_ty;
        let link_moonbitrun;
        match self.opt.target_backend {
            RunBackend::Wasm | RunBackend::WasmGC | RunBackend::Js => {
                panic!("Runtime compilation is not applicable for non-native backends")
            }
            RunBackend::Native | RunBackend::Llvm => {
                output_ty = CCOutputType::Object;
                link_moonbitrun = true;
            }
            RunBackend::NativeTccRun => {
                output_ty = CCOutputType::SharedLib;
                link_moonbitrun = false;
            }
        };

        let resolved_cc = resolve_cc(CC::default(), None);
        let libbacktrace_path = runtime_c_path.parent().unwrap().join("libbacktrace.a");
        
        let mut cc_flags = vec!["-DMOONBIT_ALLOW_STACKTRACE"];
        // Add libbacktrace.a if it exists and we're generating a shared library
        if output_ty == CCOutputType::SharedLib && libbacktrace_path.exists() {
            cc_flags.push(libbacktrace_path.to_str().unwrap());
        }

        let cc_cmd = make_cc_command_pure(
            resolved_cc,
            CCConfigBuilder::default()
                .no_sys_header(true)
                .output_ty(output_ty)
                .opt_level(CCOptLevel::Speed)
                .debug_info(true)
                .link_moonbitrun(link_moonbitrun)
                .define_use_shared_runtime_macro(false)
                .build()
                .expect("Failed to build CC configuration for runtime"),
            &cc_flags,
            [runtime_c_path.display().to_string()],
            &self.opt.target_dir_root.display().to_string(),
            Some(&artifact_path.display().to_string()),
            &self.opt.compiler_paths,
        );

        BuildCommand {
            extra_inputs: vec![runtime_c_path],
            commandline: cc_cmd.into(),
        }
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn lower_generate_mbti(&mut self, target: BuildTarget) -> BuildCommand {
        let input =
            self.layout
                .mi_of_build_target(self.packages, &target, self.opt.target_backend.into());
        let output =
            self.layout
                .generated_mbti_path(self.packages, &target, self.opt.target_backend.into());

        let cmd = Mooninfo {
            mi_in: input.into(),
            out: output.into(),
            no_alias: self.opt.info_no_alias,
        };

        BuildCommand {
            extra_inputs: vec![],
            commandline: cmd.build_command(&*moonutil::BINARIES.mooninfo).into(),
        }
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn lower_build_docs(&self) -> BuildCommand {
        // TODO: How to enforce the `packages.json` dependency is generated
        // up-to-date before the command is executed?
        //
        // If we forgot to generate anything at all, we can get a complaint from
        // n2 for the file doesn't exist and nobody can create it, but if we
        // have a stale file, we currently have to rely on ourselves.
        //
        // One possible solution is to modify `n2` to support build steps that
        // execute an in-process callback to generate files.

        // Currently, moondoc only support a single module in scope, so we
        // have these constraints
        let main_module = self
            .opt
            .main_module
            .as_ref()
            .expect("Currently only one module in the workspace is supported.");
        let path = match main_module.source() {
            ModuleSourceKind::Local(p) => p,
            ModuleSourceKind::Registry(_)
            | ModuleSourceKind::Git(_)
            | ModuleSourceKind::Stdlib(_) => {
                panic!("Remote modules for docs are not supported")
            }
            ModuleSourceKind::SingleFile(_) => {
                panic!("Single file modules for docs are not supported")
            }
        };

        let packages_json = self.layout.packages_json_path();
        let cmd = MoondocCommand::new(
            path,
            self.layout.doc_dir(),
            self.opt.stdlib_path.as_ref(),
            &packages_json,
            self.opt.docs_serve,
        );

        BuildCommand {
            commandline: cmd.build_command(&*moonutil::BINARIES.moondoc).into(),
            extra_inputs: vec![packages_json],
        }
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn lower_run_prebuild(&self, pkg: PackageId, idx: u32) -> BuildCommand {
        let info = self
            .build_plan
            .get_prebuild_info(pkg, idx)
            .expect("Prebuild info should be populated before lowering run prebuild");

        // Note: we are tracking dependencies between prebuild commands via n2.
        // Ideally we can do this ourselves, but n2 does it anyway so we don't bother.

        BuildCommand {
            commandline: Commandline::Verbatim(info.command.clone()),
            extra_inputs: info.resolved_inputs.clone(),
        }
    }

    pub(super) fn lower_moon_lex_prebuild(&self, pkg: PackageId, idx: u32) -> BuildCommand {
        let pkg = self.packages.get_package(pkg);
        let mbtlex_path = pkg
            .mbt_lex_files
            .get(idx as usize)
            .expect("mbt_lex file index out of bounds")
            .clone();
        let output = mbtlex_path.with_extension("mbt");

        let commandline = vec![
            moonutil::BINARIES.moonrun.display().to_string(),
            moonutil::BINARIES.moonlex.display().to_string(),
            "--".into(),
            mbtlex_path.display().to_string(),
            "-o".into(),
            output.display().to_string(),
        ];

        BuildCommand {
            commandline: commandline.into(),
            extra_inputs: vec![mbtlex_path],
        }
    }

    pub(super) fn lower_moon_yacc_prebuild(&self, pkg: PackageId, idx: u32) -> BuildCommand {
        let pkg = self.packages.get_package(pkg);
        let mby_path = pkg
            .mbt_yacc_files
            .get(idx as usize)
            .expect("mbt_yacc file index out of bounds")
            .clone();
        let output = mby_path.with_extension("mbt");

        let commandline = vec![
            moonutil::BINARIES.moonrun.display().to_string(),
            moonutil::BINARIES.moonyacc.display().to_string(),
            "--".into(),
            mby_path.display().to_string(),
            "-o".into(),
            output.display().to_string(),
        ];

        BuildCommand {
            commandline: commandline.into(),
            extra_inputs: vec![mby_path],
        }
    }
}
