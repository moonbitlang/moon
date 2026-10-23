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

//! Lowering implementation for auxiliary build steps.

use moonutil::{
    compiler_flags::{
        ArchiverConfigBuilder, CCConfigBuilder, OptLevel as CCOptLevel, OutputType as CCOutputType,
        make_cc_command_resolved,
    },
    resolution::{ModuleId, ModuleSourceKind},
    test_metadata::DriverKind,
    toolchain::BINARIES,
};
use tracing::{Level, instrument};

use crate::{
    build_lower::compiler::{CmdlineAbstraction, MoondocCommand, Mooninfo},
    build_plan::{ArtifactKey, BuildRuntimeInfo, BuildTargetInfo, PrebuildInfo},
    model::{BuildTarget, PackageId, TargetKind},
};

use super::{BuildCommand, LoweringError, compiler, context::ActionArtifacts, moonc_command};

const LEGACY_ASYNC_RUNTIME_PACKAGE: &str = "moonbitlang/async";
const CORE_ASYNC_RUNTIME_PACKAGE: &str = "moonbitlang/core/async";

fn select_async_runtime_package<'a>(
    package_name: &str,
    mut imports: impl Iterator<Item = &'a str>,
) -> &'static str {
    if package_name == LEGACY_ASYNC_RUNTIME_PACKAGE
        || imports.any(|import| import == LEGACY_ASYNC_RUNTIME_PACKAGE)
    {
        LEGACY_ASYNC_RUNTIME_PACKAGE
    } else {
        CORE_ASYNC_RUNTIME_PACKAGE
    }
}

impl<'a> super::LoweringContext<'a> {
    #[instrument(level = Level::DEBUG, skip(self, artifacts))]
    pub(super) fn lower_generate_node_test_package_config(
        &self,
        artifacts: &ActionArtifacts,
        package: PackageId,
    ) -> BuildCommand {
        let output = artifacts.single_output_path_matching(|artifact| {
            matches!(
                artifact,
                ArtifactKey::NodeTestPackageConfig {
                    package: artifact_package
                } if *artifact_package == package
            )
        });
        let commandline = compiler::MoonGenerateNodeTestPackageConfig {
            output: output.into(),
        }
        .build_command(&*BINARIES.moonbuild);

        BuildCommand {
            commandline: commandline.into(),
            extra_inputs: vec![],
        }
    }

    #[instrument(level = Level::DEBUG, skip(self, artifacts, info))]
    pub(super) fn lower_gen_test_driver(
        &mut self,
        artifacts: &ActionArtifacts,
        target: BuildTarget,
        info: &BuildTargetInfo,
    ) -> BuildCommand {
        let package = self.get_package(target);
        let output_driver = artifacts.single_output_path_matching(|artifact| {
            matches!(
                artifact,
                ArtifactKey::GeneratedTestDriver { package, target_kind }
                    if *package == target.package && *target_kind == target.kind
            )
        });
        let output_metadata = artifacts.single_output_path_matching(|artifact| {
            matches!(
                artifact,
                ArtifactKey::GeneratedTestMetadata { package, target_kind }
                    if *package == target.package && *target_kind == target.kind
            )
        });
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

        let target_specific_imports = match target.kind {
            TargetKind::WhiteboxTest => package.raw.wbtest_imports.as_slice(),
            TargetKind::BlackboxTest => package.raw.test_imports.as_slice(),
            TargetKind::InlineTest => &[],
            TargetKind::Source | TargetKind::SubPackage => unreachable!(),
        };
        let async_runtime_package = select_async_runtime_package(
            &pkg_full_name,
            package
                .raw
                .imports
                .iter()
                .chain(target_specific_imports)
                .map(|import| import.get_path()),
        );

        let (enable_coverage, self_coverage) =
            self.get_coverage_flags(target, package, &package.fqn, false);

        let cmd = compiler::MoonGenTestDriver {
            files: &files_vec,
            doctest_only_files: &info.doctest_files,
            output_driver: output_driver.into(),
            output_metadata: output_metadata.into(),
            bench: self.opt.action == moonutil::build_options::RunMode::Bench,
            enable_coverage,
            coverage_package_override: if self_coverage { Some("@self") } else { None },
            driver_kind,
            target_backend: self.opt.backend.target_backend(),
            patch_file,
            pkg_name: &pkg_full_name,
            max_concurrent_tests: package.raw.max_concurrent_tests,
            async_runtime_package,
        };

        let commandline = cmd.build_command(&*BINARIES.moonbuild);

        // The generated driver embeds moon's private test-driver protocol, so
        // cached driver artifacts must invalidate when the moon binary changes.
        // Common command lowering records the executable as an input.
        let mut extra_inputs = files_vec;
        extra_inputs.extend_from_slice(&info.doctest_files);
        if let Some(patch_file) = &info.patch_file {
            extra_inputs.push(patch_file.clone());
        }

        BuildCommand {
            commandline: commandline.into(),
            extra_inputs,
        }
    }

    #[instrument(level = Level::DEBUG, skip(self, artifacts))]
    pub(super) fn lower_bundle(
        &mut self,
        artifacts: &ActionArtifacts,
        module_id: ModuleId,
        targets: &[BuildTarget],
    ) -> Result<BuildCommand, LoweringError> {
        let output = artifacts.single_output_path_matching(|artifact| {
            matches!(
                artifact,
                ArtifactKey::BundleResult {
                    module: artifact_module,
                    ..
                } if *artifact_module == module_id
            )
        });

        let mut inputs = vec![];
        for dep in targets {
            inputs.push(artifacts.single_dependency_path_matching(|artifact| {
                matches!(
                    artifact,
                    ArtifactKey::CoreIr {
                        package,
                        target_kind,
                    } if *package == dep.package && *target_kind == dep.kind
                )
            }));
        }

        let cmd = compiler::MooncBundleCore::new(&inputs, output.clone());
        let commandline = moonc_command::lower(cmd.build_command(&*BINARIES.moonc), &output)?;

        Ok(BuildCommand {
            extra_inputs: vec![],
            commandline,
        })
    }

    #[instrument(level = Level::DEBUG, skip(self, artifacts))]
    pub(super) fn lower_compile_runtime_object(
        &mut self,
        artifacts: &ActionArtifacts,
        index: u32,
        info: &BuildRuntimeInfo,
    ) -> BuildCommand {
        let compiler_paths = self
            .opt
            .backend
            .compiler_paths()
            .expect("native lowering requires compiler paths");
        let artifact_path = artifacts.single_output_path_matching(|artifact| {
            matches!(artifact, ArtifactKey::RuntimeObject { .. })
        });
        let source = &info.source_files[index as usize];

        let runtime_toolchain = &info.effective_native_toolchain;
        let resolved_cc = runtime_toolchain.cc().clone();
        let (cc_cmd, command_env) = if let Some(crt) = runtime_toolchain.msvc_crt_policy() {
            (
                compiler::msvc::compile_runtime_command(
                    runtime_toolchain,
                    source,
                    &artifact_path,
                    &compiler_paths.include_path,
                    crt,
                    info.native_allocator,
                    self.opt.backend.collect_ref_cycle(),
                )
                .into(),
                compiler::msvc::command_env(runtime_toolchain),
            )
        } else {
            (
                make_cc_command_resolved(
                    resolved_cc,
                    CCConfigBuilder::default()
                        .no_sys_header(true)
                        .output_ty(CCOutputType::Object)
                        .opt_level(CCOptLevel::Speed)
                        .debug_info(true)
                        .allow_stacktrace(info.enable_backtrace)
                        .link_moonbitrun(true)
                        .define_use_shared_runtime_macro(false)
                        .use_simdutf(!info.simdutf_objects.is_empty())
                        .native_allocator(info.native_allocator)
                        .collect_ref_cycle(self.opt.backend.collect_ref_cycle())
                        .build()
                        .expect("Failed to build CC configuration for runtime"),
                    &[] as &[&str],
                    [source.display().to_string()],
                    &self
                        .artifact_paths
                        .target_layout()
                        .runtime_output_dir(self.opt.backend.target_backend())
                        .display()
                        .to_string(),
                    Some(&artifact_path.display().to_string()),
                    compiler_paths,
                )
                .into(),
                Vec::new(),
            )
        };

        BuildCommand {
            extra_inputs: vec![source.clone()],
            commandline: cc_cmd,
        }
        .with_env(command_env)
    }

    #[instrument(level = Level::DEBUG, skip(self, artifacts, info))]
    pub(super) fn lower_build_runtime_lib(
        &mut self,
        artifacts: &ActionArtifacts,
        info: &BuildRuntimeInfo,
    ) -> BuildCommand {
        let artifact_path = artifacts.single_output_path_matching(|artifact| {
            matches!(artifact, ArtifactKey::RuntimeLibrary)
        });

        let mut object_files = artifacts.dependency_paths_matching(|artifact| {
            matches!(artifact, ArtifactKey::RuntimeObject { .. })
        });
        object_files.extend(info.simdutf_objects.iter().cloned());
        let config = ArchiverConfigBuilder::default()
            .archive_moonbitrun(false)
            .build()
            .expect("Failed to build archiver configuration");
        let mut command = BuildCommand::archive(
            &info.effective_native_toolchain,
            config,
            &object_files,
            &artifact_path,
            self.opt
                .backend
                .compiler_paths()
                .expect("native lowering requires compiler paths"),
        );
        command
            .extra_inputs
            .extend(info.simdutf_objects.iter().cloned());
        command
    }

    #[instrument(level = Level::DEBUG, skip(self, artifacts))]
    pub(super) fn lower_generate_mbti(
        &mut self,
        artifacts: &ActionArtifacts,
        target: BuildTarget,
    ) -> BuildCommand {
        let input = artifacts.single_dependency_path_matching(|artifact| {
            matches!(
                artifact,
                ArtifactKey::CheckMi {
                    package,
                    target_kind,
                } if *package == target.package && *target_kind == target.kind
            )
        });
        let output = artifacts.single_output_path_matching(|artifact| {
            matches!(
                artifact,
                ArtifactKey::GeneratedMbti { package, target_kind }
                    if *package == target.package && *target_kind == target.kind
            )
        });

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
            ModuleSourceKind::Registry | ModuleSourceKind::Stdlib(_) => {
                panic!("Remote modules for docs are not supported")
            }
            ModuleSourceKind::SingleFile(_) => {
                panic!("Single file modules for docs are not supported")
            }
        };

        let packages_json = self
            .artifact_paths
            .target_layout()
            .packages_json_path(self.opt.backend.target_backend());
        let cmd = MoondocCommand::new(
            path,
            self.artifact_paths.target_layout().doc_dir(),
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
    pub(super) fn lower_run_prebuild(
        &self,
        info: &PrebuildInfo,
    ) -> Result<BuildCommand, LoweringError> {
        // Note: we are tracking dependencies between prebuild commands via n2.
        // Ideally we can do this ourselves, but n2 does it anyway so we don't bother.

        let commandline = if let Some(embed_args) = info.command.strip_prefix(":embed ") {
            let mut commandline = vec![
                dunce::simplified(&BINARIES.moonbuild).display().to_string(),
                "tool".to_string(),
                "embed".to_string(),
            ];
            commandline.extend(
                moonutil::shlex::split_native_args(embed_args)
                    .ok_or(LoweringError::MalformedEmbedArguments)?,
            );
            commandline
        } else {
            vec![
                dunce::simplified(&BINARIES.moonbuild).display().to_string(),
                "tool".to_string(),
                "exec".to_string(),
                "--shell".to_string(),
                info.command.clone(),
            ]
        };

        Ok(BuildCommand {
            commandline: commandline.into(),
            extra_inputs: info.resolved_inputs.clone(),
        }
        .with_cwd(info.cwd.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CORE_ASYNC_RUNTIME_PACKAGE, LEGACY_ASYNC_RUNTIME_PACKAGE, select_async_runtime_package,
    };

    #[test]
    fn selects_async_runtime_package_from_imports() {
        assert_eq!(
            select_async_runtime_package("example/pkg", [LEGACY_ASYNC_RUNTIME_PACKAGE].into_iter()),
            LEGACY_ASYNC_RUNTIME_PACKAGE
        );
        assert_eq!(
            select_async_runtime_package(LEGACY_ASYNC_RUNTIME_PACKAGE, [].into_iter()),
            LEGACY_ASYNC_RUNTIME_PACKAGE
        );
        assert_eq!(
            select_async_runtime_package(
                "example/pkg",
                ["moonbitlang/async/internal/event_loop"].into_iter(),
            ),
            CORE_ASYNC_RUNTIME_PACKAGE
        );
    }
}
