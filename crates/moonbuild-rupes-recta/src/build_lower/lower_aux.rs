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
        CCConfigBuilder, OptLevel as CCOptLevel, OutputType as CCOutputType,
        make_cc_command_resolved,
    },
    mooncakes::{ModuleId, ModuleSourceKind},
    toolchain::BINARIES,
};
use tracing::{Level, instrument};

use crate::{
    build_action_plan::{BuildActionId, PlannedArtifact},
    build_lower::{
        Commandline,
        compiler::{CmdlineAbstraction, MoondocCommand, Mooninfo},
    },
    build_plan::{BuildTargetInfo, PrebuildInfo},
    model::{BuildTarget, OperatingSystem, PackageId, TargetKind},
};

use super::{BuildCommand, compiler};

impl<'a> super::LoweringContext<'a> {
    #[instrument(level = Level::DEBUG, skip(self, info))]
    pub(super) fn lower_gen_test_driver(
        &mut self,
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

        let commandline = cmd.build_command(&*BINARIES.moonbuild);

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
        module_id: ModuleId,
        targets: &[BuildTarget],
    ) -> BuildCommand {
        let module = self.modules.module_source(module_id);
        let output = self
            .layout
            .bundle_result_path(self.opt.target_backend.into(), module.name());

        let mut inputs = vec![];
        for dep in targets {
            inputs.push(self.layout.core_of_build_target(
                self.packages,
                dep,
                self.opt.target_backend.into(),
            ));
        }

        let cmd = compiler::MooncBundleCore::new(&inputs, output);

        BuildCommand {
            extra_inputs: vec![],
            commandline: cmd.build_command(&*BINARIES.moonc).into(),
        }
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn lower_compile_runtime(
        &mut self,
        action: BuildActionId,
    ) -> anyhow::Result<BuildCommand> {
        let artifact_path =
            self.single_artifact_path(&PlannedArtifact::RuntimeLib { producer: action });

        let runtime_c_path = self.opt.runtime_dot_c_path();

        let use_shared_runtime = self.opt.selected_backend.uses_shared_runtime();
        let (output_ty, link_moonbitrun) = if use_shared_runtime {
            (CCOutputType::SharedLib, false)
        } else {
            (CCOutputType::Object, true)
        };

        let resolved_cc = moonutil::compiler_flags::default_native_toolchain(
            self.opt
                .tcc_run
                .as_ref()
                .map(|config| config.internal_tcc()),
        )?
        .cc()
        .clone();
        let use_simdutf = !use_shared_runtime
            && resolved_cc.can_use_simdutf()
            && self.opt.compiler_paths().simdutf_object_paths().is_some();

        let cc_cmd = make_cc_command_resolved(
            resolved_cc,
            CCConfigBuilder::default()
                .no_sys_header(true)
                .output_ty(output_ty)
                .opt_level(CCOptLevel::Speed)
                .debug_info(true)
                .allow_stacktrace(
                    self.opt.debug_symbols && self.opt.os() != OperatingSystem::Windows,
                )
                .define_tinyc_macro(use_shared_runtime)
                .preserve_frame_pointer(use_shared_runtime)
                .link_moonbitrun(link_moonbitrun)
                .link_libbacktrace(output_ty == CCOutputType::SharedLib)
                .define_use_shared_runtime_macro(false)
                .use_simdutf(use_simdutf)
                .build()
                .expect("Failed to build CC configuration for runtime"),
            &[] as &[&str],
            [runtime_c_path.display().to_string()],
            &self.opt.target_dir_root.display().to_string(),
            Some(&artifact_path.display().to_string()),
            self.opt.compiler_paths(),
        );

        Ok(BuildCommand {
            extra_inputs: vec![runtime_c_path],
            commandline: cc_cmd.into(),
        })
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
            commandline: cmd.build_command(&*BINARIES.mooninfo).into(),
        }
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn lower_build_docs(&self, module_id: ModuleId) -> BuildCommand {
        // TODO: How to enforce the `packages.json` dependency is generated
        // up-to-date before the command is executed?
        //
        // If we forgot to generate anything at all, we can get a complaint from
        // n2 for the file doesn't exist and nobody can create it, but if we
        // have a stale file, we currently have to rely on ourselves.
        //
        // One possible solution is to modify `n2` to support build steps that
        // execute an in-process callback to generate files.

        let module = self.modules.module_source(module_id);
        let path = match module.source() {
            ModuleSourceKind::Local(path) => path,
            ModuleSourceKind::Registry | ModuleSourceKind::Git(_) | ModuleSourceKind::Stdlib(_) => {
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
            commandline: cmd.build_command(&*BINARIES.moondoc).into(),
            extra_inputs: vec![packages_json],
        }
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn lower_run_prebuild(&self, info: &PrebuildInfo) -> BuildCommand {
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
            BINARIES.moonrun.display().to_string(),
            BINARIES.moonlex.display().to_string(),
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
            BINARIES.moonrun.display().to_string(),
            BINARIES.moonyacc.display().to_string(),
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
