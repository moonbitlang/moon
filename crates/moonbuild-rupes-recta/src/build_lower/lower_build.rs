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

//! Lowering implementation for build actions.

use std::{
    collections::{BTreeSet, HashSet},
    path::{Path, PathBuf},
};

use moonutil::{
    build_options::RunMode,
    compiler_flags::{
        ArchiverConfigBuilder, CCConfigBuilder, LinkerConfigBuilder, OutputType as CCOutputType,
        Toolchain, make_cc_command_resolved_for_toolchain, make_linker_command_resolved,
    },
    cond_expr::OptLevel,
    package::JsFormat,
    resolution::{CORE_MODULE, ModuleId},
    target::TargetBackend,
    toolchain::BINARIES,
};
use petgraph::Direction;
use tracing::{Level, instrument};

use crate::{
    build_lower::{
        WarningCondition,
        compiler::{
            BuildCommonConfig, BuildCommonInput, CmdlineAbstraction, ErrorFormat, JsConfig,
            MiDependency, PackageSource, WasmConfig,
        },
    },
    build_plan::{
        ArtifactKey, BuildCStubsInfo, BuildRuntimeInfo, BuildTargetInfo, CompileGeneratedCInfo,
        ForeignLibraryInfo, LinkCoreInfo, MakeExecutableInfo,
    },
    discover::DiscoveredPackage,
    model::{BackendConfig, BuildTarget, NativeBackendMode, PackageId, TargetKind},
    pkg_name::{PackageFQN, PackagePath},
    special_cases::{is_self_coverage_lib, should_skip_coverage},
    target_layout::proof_artifact_stem,
};

use super::{
    BuildCommand, LoweringError, compiler,
    context::{ActionArtifacts, LoweringContext},
    moonc_command,
};

impl<'a> LoweringContext<'a> {
    fn compiler_source_files(&self, info: &BuildTargetInfo) -> Vec<PathBuf> {
        let mut files = info.files().map(|x| x.to_owned()).collect::<Vec<_>>();
        files.extend(info.mbtp_files().map(|x| x.to_owned()));
        files
    }

    fn is_module_third_party(&self, mid: ModuleId) -> bool {
        // This is usually a small vector, so this perf overhead is okay.
        !self.modules.input_module_ids().contains(&mid)
    }

    pub(super) fn set_flags(&self) -> compiler::CompilationFlags {
        let debug_info = self.plan.backend_plan().moonc_debug_info();
        compiler::CompilationFlags {
            no_opt: self.opt.opt_level == OptLevel::Debug,
            symbols: debug_info,
            stacktrace: self.plan.backend_plan().moonc_stacktrace(),
            source_map: self.opt.backend.target_backend().supports_source_map() && debug_info,
            enable_coverage: false,
            self_coverage: false,
            enable_value_tracing: false,
        }
    }

    /// Returns `(enable_coverage, self_coverage)` for the given conditions
    ///
    /// - `target`: The build target, to determine if it's a blackbox test (bb
    ///   tests builds themselves don't need coverage)
    /// - `package`: The discovered package, to check if it's third-party
    /// - `fqn`: The package FQN, to match against hardcoded exceptions
    /// - `is_build`: Whether this is a build command. BB tests need to be aware
    ///   of coverage but not apply it to themselves.
    pub(super) fn get_coverage_flags(
        &self,
        target: BuildTarget,
        package: &DiscoveredPackage,
        fqn: &PackageFQN,
        is_build: bool,
    ) -> (bool, bool) {
        let enable_coverage = self.opt.enable_coverage
            && (!is_build || target.kind != TargetKind::BlackboxTest)
            && !should_skip_coverage(fqn)
            && !self.is_module_third_party(package.module);
        let self_coverage = enable_coverage && is_self_coverage_lib(fqn);
        (enable_coverage, self_coverage)
    }

    fn set_build_commons(
        &self,
        artifacts: &ActionArtifacts,
        pkg: &DiscoveredPackage,
        info: &'a BuildTargetInfo,
        is_main: bool,
    ) -> BuildCommonConfig<'a> {
        // Warning and error settings
        let error_format = if self.opt.moonc_output_json {
            ErrorFormat::Json
        } else {
            ErrorFormat::Regular
        };
        let deny_warn = self.opt.warning_condition == WarningCondition::Deny;
        let allow_warn = self.opt.warning_condition == WarningCondition::Allow;

        let warn_config = if self.is_module_third_party(pkg.module) || allow_warn {
            // Third-party modules don't have any warnings enabled explicitly.
            compiler::WarnAlertConfig::Suppress
        } else if let Some(w) = &info.warn_list {
            compiler::WarnAlertConfig::List(w.into())
        } else {
            compiler::WarnAlertConfig::default()
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

        // BuildTargetInfo selects the virtual-package flag and alias behavior;
        // the artifact dependency remains the sole authority for its `.mi`
        // path.
        let mut virtual_implementation = None;
        let mut check_mi = None;
        let no_mi = info.no_mi();

        if let Some(v_target) = info.check_mi_against {
            let mi_path = if self.packages.is_stdlib_package(v_target.package) {
                // Installed stdlib virtual contracts are known toolchain
                // inputs, not artifacts produced by this Build Plan.
                self.artifact_paths.mi_of_build_target(
                    self.packages,
                    &v_target,
                    self.opt.backend.target_backend(),
                )
            } else {
                artifacts.single_dependency_path_matching(|artifact| {
                    matches!(
                        artifact,
                        ArtifactKey::VirtualContractMi { package }
                            if *package == v_target.package
                    )
                })
            };

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

            // Implementation package will generate a dummy mi file so that it
            // won't be rebuilt every time
        }

        // Map the effective `is_main` (already accounts for test targets, which
        // link their own `main`) plus force-link into the package kind moonc
        // consumes via `-pkgtype`.
        use moonutil::package::PackageKind;
        let pkgtype = if is_main {
            PackageKind::Executable
        } else if pkg.raw.force_link {
            PackageKind::ForeignLibrary
        } else {
            PackageKind::Library
        };

        BuildCommonConfig {
            error_format,
            deny_warn,
            warn_config,
            patch_file,
            no_mi,
            workspace_root,
            pkgtype,
            use_async_io: pkg.raw.use_async_io,

            check_mi,
            virtual_implementation,
            value_tracing: info.value_tracing,
        }
    }

    fn extend_extra_inputs(
        &self,
        info: &BuildTargetInfo,
        commons: &BuildCommonConfig<'a>,
        mi_inputs: &MiInputs<'a>,
        extra_inputs: &mut Vec<PathBuf>,
    ) {
        extra_inputs.extend(mi_inputs.installed_stdlib.iter().cloned());
        // Local virtual interfaces are already observed by their artifact
        // dependency. Installed stdlib contracts have no provider in this
        // plan, so observe the exact `-check-mi`/`-impl-virtual` file.
        if let Some(target) = info.check_mi_against
            && self.packages.is_stdlib_package(target.package)
        {
            let path = match (&commons.check_mi, &commons.virtual_implementation) {
                (Some(path), None) => path.as_ref(),
                (None, Some(implementation)) => implementation.mi_path.as_ref(),
                _ => unreachable!("virtual validation should select exactly one interface path"),
            };
            if !extra_inputs.iter().any(|input| input == path) {
                extra_inputs.push(path.to_path_buf());
            }
        }
        if let Some(patch) = &commons.patch_file {
            extra_inputs.push(patch.as_ref().to_path_buf());
        }
    }

    fn dep_proofs_of(&self, target: BuildTarget) -> Vec<compiler::DepProof<'a>> {
        let mut deps = self
            .rel
            .dep_graph
            .edges_directed(target, Direction::Outgoing)
            // Stdlib deps do not currently participate in `--dep-proof`
            // mapping: their interfaces are consumed via explicit stdlib `.mi`
            // paths, and we do not emit/load stdlib proof modules under
            // `_build/verif`. If stdlib later grows proof-related artifacts of
            // its own, this filter and the surrounding lowering logic should be
            // revisited.
            .filter(|(_, dep, _)| !self.packages.is_stdlib_package(dep.package))
            .map(|(_, dep, _)| {
                let pkg = self.packages.get_package(dep.package);
                compiler::DepProof::new(pkg.fqn.to_string(), proof_artifact_stem(&pkg.fqn))
            })
            .collect::<Vec<_>>();
        deps.sort_by(|x, y| x.package.cmp(&y.package));
        deps
    }

    fn proof_loadpaths_of(&self, target: BuildTarget, proof_prelude: &Path) -> Vec<PathBuf> {
        let mut visited = HashSet::new();
        let mut pending = vec![target];
        let mut loadpaths = BTreeSet::new();

        loadpaths.insert(proof_prelude.to_path_buf());

        while let Some(current) = pending.pop() {
            if !visited.insert(current) {
                continue;
            }

            for dep in self
                .rel
                .dep_graph
                .neighbors_directed(current, Direction::Outgoing)
            {
                if self.packages.is_stdlib_package(dep.package) {
                    continue;
                }

                loadpaths.insert(
                    self.artifact_paths
                        .target_layout()
                        .verif_package_dir(self.packages, &dep),
                );
                pending.push(dep);
            }
        }

        loadpaths.into_iter().collect()
    }

    #[instrument(level = Level::DEBUG, skip(self, artifacts, info))]
    pub(super) fn lower_check(
        &self,
        artifacts: &ActionArtifacts,
        target: BuildTarget,
        info: &BuildTargetInfo,
    ) -> Result<BuildCommand, LoweringError> {
        let package = self.get_package(target);
        let module = self.packages.module_info(package.module);

        let mi_output = artifacts
            .optional_single_output_path_matching(|artifact| {
                matches!(
                    artifact,
                    ArtifactKey::CheckMi {
                        package,
                        target_kind,
                    } if *package == target.package && *target_kind == target.kind
                )
            })
            .unwrap_or_else(|| {
                if info.check_mi_against.is_some() {
                    self.artifact_paths
                        .mi_of_build_target_impl_virtual(
                            self.packages,
                            &target,
                            self.opt.backend.target_backend(),
                        )
                        .into_path()
                } else {
                    unreachable!("regular Check actions should have one package interface artifact")
                }
            });
        let mi_inputs = self.mi_inputs_of(artifacts, target);

        // Collect files iterator once so we can pass slices and extra inputs
        let files_vec = self.compiler_source_files(info);

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
        let backend = self.opt.backend.target_backend();
        let cmd = compiler::MooncCheck {
            required: BuildCommonInput::new(
                &files_vec,
                &info.doctest_files,
                &mi_inputs.deps,
                compiler::CompiledPackageName::new(&package.fqn, target.kind),
                &package.root_path,
                self.artifact_paths
                    .target_layout()
                    .all_pkgs_of_build_target(backend),
                backend,
                target.kind,
            ),
            defaults: self.set_build_commons(artifacts, package, info, is_main),
            mi_out: mi_output.into(),
            single_file: package.is_single_file(),
            ignore_import_declaration: package.is_mbtx_single_file(),
            extra_flags: module.compile_flags.as_deref().unwrap_or_default(),
        };

        // Track doctest-only files as inputs as well
        let mut extra_inputs = files_vec.clone();
        extra_inputs.extend(info.doctest_files.clone());
        if !package.is_single_file() {
            extra_inputs.push(package.config_path());
        }

        self.extend_extra_inputs(info, &cmd.defaults, &mi_inputs, &mut extra_inputs);

        let commandline =
            moonc_command::lower(cmd.build_command(&*BINARIES.moonc), cmd.mi_out.as_ref())?;

        Ok(BuildCommand {
            extra_inputs,
            commandline,
        })
    }

    #[instrument(level = Level::DEBUG, skip(self, artifacts, info))]
    pub(super) fn lower_emit_proof(
        &self,
        artifacts: &ActionArtifacts,
        target: BuildTarget,
        info: &BuildTargetInfo,
    ) -> Result<BuildCommand, LoweringError> {
        let package = self.get_package(target);
        let module = self.packages.module_info(package.module);
        let mi_inputs = self.mi_inputs_of(artifacts, target);

        let files_vec = self.compiler_source_files(info);

        let backend = self.opt.backend.target_backend();
        let whyml_output = artifacts.single_output_path_matching(|artifact| {
            matches!(
                artifact,
                ArtifactKey::ProofWhyml { package, target_kind }
                    if *package == target.package && *target_kind == target.kind
            )
        });
        let dep_proofs = self.dep_proofs_of(target);
        let cmd = compiler::MooncProve {
            required: BuildCommonInput::new(
                &files_vec,
                &info.doctest_files,
                &mi_inputs.deps,
                compiler::CompiledPackageName::new(&package.fqn, target.kind),
                &package.root_path,
                self.artifact_paths
                    .target_layout()
                    .all_pkgs_of_build_target(backend),
                backend,
                target.kind,
            ),
            defaults: self.set_build_commons(artifacts, package, info, package.raw.is_main),
            whyml_out: whyml_output.clone().into(),
            proof_report_out: None,
            why3_config: None,
            why3_loadpaths: Vec::new(),
            dep_proofs,
            emit_only: true,
            single_file: package.is_single_file(),
            extra_flags: module.compile_flags.as_deref().unwrap_or_default(),
        };

        let mut extra_inputs = files_vec.clone();
        extra_inputs.extend(info.doctest_files.clone());
        if !package.is_single_file() {
            extra_inputs.push(package.config_path());
        }
        self.extend_extra_inputs(info, &cmd.defaults, &mi_inputs, &mut extra_inputs);

        let commandline = moonc_command::lower(
            cmd.build_command(compiler::ProveDriver::from_toolchain()),
            &whyml_output,
        )?;

        Ok(BuildCommand {
            extra_inputs,
            commandline,
        })
    }

    #[instrument(level = Level::DEBUG, skip(self, artifacts, info))]
    pub(super) fn lower_prove(
        &self,
        artifacts: &ActionArtifacts,
        target: BuildTarget,
        info: &BuildTargetInfo,
    ) -> Result<BuildCommand, LoweringError> {
        let package = self.get_package(target);
        let module = self.packages.module_info(package.module);
        let mi_inputs = self.mi_inputs_of(artifacts, target);

        let files_vec = self.compiler_source_files(info);

        let backend = self.opt.backend.target_backend();
        let why3_config = info
            .why3_config
            .clone()
            .unwrap_or_else(|| self.artifact_paths.target_layout().why3_config_path());
        let whyml_output = artifacts.single_output_path_matching(|artifact| {
            matches!(
                artifact,
                ArtifactKey::ProofWhyml { package, target_kind }
                    if *package == target.package && *target_kind == target.kind
            )
        });
        let proof_report_output = artifacts.single_output_path_matching(|artifact| {
            matches!(
                artifact,
                ArtifactKey::ProofReport { package, target_kind }
                    if *package == target.package && *target_kind == target.kind
            )
        });
        let dep_proofs = self.dep_proofs_of(target);
        let proof_prelude = info.proof_prelude.as_path();
        // Why3 needs every reachable non-stdlib proof directory on its loadpath
        // because emitted proof artifacts live alongside each package's
        // verification outputs.
        let why3_loadpaths = self.proof_loadpaths_of(target, proof_prelude);
        let cmd = compiler::MooncProve {
            required: BuildCommonInput::new(
                &files_vec,
                &info.doctest_files,
                &mi_inputs.deps,
                compiler::CompiledPackageName::new(&package.fqn, target.kind),
                &package.root_path,
                self.artifact_paths
                    .target_layout()
                    .all_pkgs_of_build_target(backend),
                backend,
                target.kind,
            ),
            defaults: self.set_build_commons(artifacts, package, info, package.raw.is_main),
            whyml_out: whyml_output.clone().into(),
            proof_report_out: Some(proof_report_output.clone().into()),
            why3_config: Some(why3_config.clone().into()),
            why3_loadpaths: why3_loadpaths.clone(),
            dep_proofs,
            emit_only: false,
            single_file: package.is_single_file(),
            extra_flags: module.compile_flags.as_deref().unwrap_or_default(),
        };

        let mut extra_inputs = files_vec.clone();
        extra_inputs.extend(info.doctest_files.clone());
        extra_inputs.push(why3_config);
        extra_inputs.push(proof_prelude.join(moonutil::constants::PRELUDE_PROOF_FILE));
        extra_inputs.extend(why3_loadpaths);
        if !package.is_single_file() {
            extra_inputs.push(package.config_path());
        }
        self.extend_extra_inputs(info, &cmd.defaults, &mi_inputs, &mut extra_inputs);

        let commandline = moonc_command::lower(
            cmd.build_command(compiler::ProveDriver::from_toolchain()),
            &whyml_output,
        )?;

        Ok(BuildCommand {
            extra_inputs,
            commandline,
        })
    }

    #[instrument(level = Level::DEBUG, skip(self, artifacts, info))]
    pub(super) fn lower_build_mbt(
        &self,
        artifacts: &ActionArtifacts,
        target: BuildTarget,
        info: &BuildTargetInfo,
    ) -> Result<BuildCommand, LoweringError> {
        let package = self.get_package(target);
        let module = self.packages.module_info(package.module);

        let core_output = artifacts.single_output_path_matching(|artifact| {
            matches!(
                artifact,
                ArtifactKey::CoreIr {
                    package,
                    target_kind,
                } if *package == target.package && *target_kind == target.kind
            )
        });
        let mi_output = artifacts
            .optional_single_output_path_matching(|artifact| {
                matches!(
                    artifact,
                    ArtifactKey::BuildMi {
                        package,
                        target_kind,
                    } if *package == target.package && *target_kind == target.kind
                )
            })
            .unwrap_or_else(|| {
                self.artifact_paths.mi_of_build_target(
                    self.packages,
                    &target,
                    self.opt.backend.target_backend(),
                )
            });

        let mi_inputs = self.mi_inputs_of(artifacts, target);

        let mut files = self.compiler_source_files(info);
        match target.kind {
            TargetKind::Source | TargetKind::SubPackage => {}
            TargetKind::WhiteboxTest | TargetKind::BlackboxTest | TargetKind::InlineTest => {
                let test_driver = artifacts.single_dependency_path_matching(|artifact| {
                    matches!(
                        artifact,
                        ArtifactKey::GeneratedTestDriver { package, target_kind }
                            if *package == target.package && *target_kind == target.kind
                    )
                });
                files.push(test_driver);
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
        let backend = self.opt.backend.target_backend();
        let mut cmd = compiler::MooncBuildPackage {
            required: BuildCommonInput::new(
                &files,
                &info.doctest_files,
                &mi_inputs.deps,
                compiler::CompiledPackageName::new(&package.fqn, target.kind),
                &package.root_path,
                self.artifact_paths
                    .target_layout()
                    .all_pkgs_of_build_target(backend),
                backend,
                target.kind,
            ),
            defaults: self.set_build_commons(artifacts, package, info, is_main),
            core_out: core_output.clone().into(),
            mi_out: mi_output.into(),
            flags: self.set_flags(),
            ignore_import_declaration: package.is_mbtx_single_file(),
            extra_build_opts: module.compile_flags.as_deref().unwrap_or_default(),
        };
        // Propagate debug/coverage flags and common settings
        (cmd.flags.enable_coverage, cmd.flags.self_coverage) =
            self.get_coverage_flags(target, package, &package.fqn, true);
        cmd.defaults.no_mi |= target.kind.is_test() | (cmd.defaults.check_mi.is_some());

        // Include doctest-only files as inputs to track dependency correctly
        // Note: This is the *extra* inputs, trivial dependencies are already
        // tracked via the build graph.
        let mut extra_inputs = self.compiler_source_files(info);
        extra_inputs.extend(info.doctest_files.clone());
        if !package.is_single_file() {
            extra_inputs.push(package.config_path());
        }

        self.extend_extra_inputs(info, &cmd.defaults, &mi_inputs, &mut extra_inputs);

        let commandline = moonc_command::lower(cmd.build_command(&*BINARIES.moonc), &core_output)?;

        Ok(BuildCommand {
            commandline,
            extra_inputs,
        })
    }

    #[instrument(level = Level::DEBUG, skip(self, artifacts, info))]
    pub(super) fn lower_link_core(
        &mut self,
        artifacts: &ActionArtifacts,
        target: BuildTarget,
        info: &LinkCoreInfo,
        native_toolchain: Option<&Toolchain>,
    ) -> Result<BuildCommand, LoweringError> {
        #[cfg(not(target_os = "windows"))]
        let _ = native_toolchain;

        let package = self.get_package(target);
        let module = self.packages.module_info(package.module);

        let backend = self.opt.backend.target_backend();
        // Installed stdlib `.core` files are not produced by any action, so
        // n2 must observe them as extra inputs.
        let mut stdlib_inputs = Vec::new();
        let mut core_input_files = Vec::new();
        // When present, the abort default must precede the other stdlib
        // packages to preserve initialization order.
        if let Some(stdlib) = &self.opt.stdlib_path
            && info.link_stdlib_abort
        {
            let abort = moonutil::toolchain::abort_core_in(stdlib, backend);
            stdlib_inputs.push(abort.clone());
            core_input_files.push(abort);
        }
        // Linked core targets, including the stdlib packages they reach
        for target in &info.linked_order {
            let core_path = if self.packages.is_stdlib_package(target.package) {
                let path = self
                    .artifact_paths
                    .core_of_build_target(self.packages, target, backend);
                stdlib_inputs.push(path.clone());
                path
            } else {
                artifacts.single_dependency_path_matching(|artifact| {
                    matches!(
                        artifact,
                        ArtifactKey::CoreIr {
                            package,
                            target_kind,
                        } if *package == target.package && *target_kind == target.kind
                    )
                })
            };
            core_input_files.push(core_path);
        }

        let out_file = artifacts.single_output_path();

        let core_fqn = PackageFQN::new(CORE_MODULE.clone(), PackagePath::empty());
        // The stdlib module is described by a single source entry below.
        let package_sources = info
            .linked_order
            .iter()
            .filter(|target| !self.packages.is_stdlib_package(target.package))
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
            output_path: out_file.clone().into(),
            pkg_config_path: config_path.clone().into(),
            package_sources: &package_sources,
            stdlib_core_source: None,
            target_backend: self.opt.backend.target_backend(),
            native_target: self.plan.backend_plan().direct_native_target(),
            flags: self.set_flags(),
            test_mode: target.kind.is_test(),
            wasm_config: self.get_wasm_config(target, package),
            js_config: self.get_js_config(target, package),
            exports: package.exported_functions(self.opt.backend.target_backend()),
            extra_link_opts: module.link_flags.as_deref().unwrap_or_default(),
            #[cfg(target_os = "windows")]
            native_toolchain_is_msvc: native_toolchain
                .is_some_and(|toolchain| toolchain.uses_msvc_abi()),
        };

        let mut extra_inputs = stdlib_inputs;
        if !package.is_single_file() {
            extra_inputs.push(config_path);
        }

        let commandline = moonc_command::lower(cmd.build_command(&*BINARIES.moonc), &out_file)?;

        Ok(BuildCommand {
            extra_inputs,
            commandline,
        })
    }

    fn get_wasm_config<'b>(
        &self,
        target: BuildTarget,
        pkg: &'b DiscoveredPackage,
    ) -> compiler::WasmConfig<'b> {
        let mut wasm_config = if self.opt.backend.target_backend() == TargetBackend::Wasm
            && let Some(cfg) = pkg.raw.link.as_ref().and_then(|x| x.wasm.as_ref())
        {
            WasmConfig {
                module_name: None,
                export_memory_name: cfg.export_memory_name.as_deref().map(|x| x.into()),
                import_memory: cfg.import_memory.as_ref(),
                memory_limits: cfg.memory_limits.as_ref(),
                shared_memory: cfg.shared_memory,
                heap_start_address: cfg.heap_start_address,
                link_flags: cfg.flags.as_deref(),
                wasi: false,
                new_allocator: false,
                collect_ref_cycle: false,
            }
        } else if self.opt.backend.target_backend() == TargetBackend::WasmGC
            && let Some(cfg) = pkg.raw.link.as_ref().and_then(|x| x.wasm_gc.as_ref())
        {
            WasmConfig {
                module_name: None,
                export_memory_name: cfg.export_memory_name.as_deref().map(|x| x.into()),
                import_memory: cfg.import_memory.as_ref(),
                memory_limits: cfg.memory_limits.as_ref(),
                shared_memory: cfg.shared_memory,
                heap_start_address: None,
                link_flags: cfg.flags.as_deref(),
                wasi: false,
                new_allocator: false,
                collect_ref_cycle: false,
            }
        } else {
            WasmConfig::default()
        };

        if let BackendConfig::Wasm {
            new_allocator,
            collect_ref_cycle,
            ..
        } = &self.opt.backend
        {
            wasm_config.new_allocator = *new_allocator;
            wasm_config.collect_ref_cycle = *collect_ref_cycle;
        }
        if self.should_link_wasi(target, pkg) {
            wasm_config.wasi = true;
        }

        if matches!(
            &self.opt.backend,
            BackendConfig::Wasm { use_wat: false, .. } | BackendConfig::WasmGc { use_wat: false }
        ) && self.opt.opt_level == OptLevel::Release
        {
            let module = self.packages.module_info(pkg.module);
            // Use the manifest's optional version directly: version resolution
            // may represent a missing version as `0.0.0` elsewhere.
            let module_name = module.version.as_ref().map_or_else(
                || pkg.fqn.to_string(),
                |version| format!("{}@{version}", pkg.fqn),
            );
            wasm_config.module_name = Some(module_name.into());
        }

        wasm_config
    }

    fn should_link_wasi(&self, target: BuildTarget, pkg: &DiscoveredPackage) -> bool {
        if !self.opt.backend.wasi_link() {
            return false;
        }

        match self.opt.action {
            RunMode::Run | RunMode::Test | RunMode::Bench => true,
            RunMode::Build => {
                matches!(target.kind, TargetKind::Source | TargetKind::SubPackage)
                    && pkg.raw.is_main
            }
            RunMode::Check | RunMode::Prove | RunMode::Bundle | RunMode::Format => false,
        }
    }

    fn get_js_config(&self, target: BuildTarget, pkg: &DiscoveredPackage) -> Option<JsConfig> {
        let backend = self.opt.backend.target_backend();
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

    #[instrument(level = Level::DEBUG, skip(self, artifacts, info))]
    pub(super) fn lower_build_c_stub(
        &mut self,
        artifacts: &ActionArtifacts,
        target: PackageId,
        index: u32,
        info: &BuildCStubsInfo,
    ) -> BuildCommand {
        if !self.opt.backend.target_backend().is_native() {
            unreachable!("C stubs are only lowered for C or LLVM backends")
        }

        let package = self.packages.get_package(target);

        let input_file = &package.c_stub_files[index as usize];
        let output_file = artifacts.single_output_path_matching(|artifact| {
            matches!(
                artifact,
                ArtifactKey::CStubObject { package, .. } if *package == target
            )
        });

        let config = CCConfigBuilder::default()
            .no_sys_header(true)
            .output_ty(CCOutputType::Object)
            .opt_level(info.opt_level)
            .debug_info(info.debug_info)
            .collect_ref_cycle(self.opt.backend.collect_ref_cycle())
            .link_moonbitrun(true)
            .define_use_shared_runtime_macro(false)
            .build()
            .expect("Failed to build CC configuration for C stub");

        let intermediate_dir = self
            .artifact_paths
            .target_layout()
            .package_dir(&package.fqn, self.opt.backend.target_backend())
            .display()
            .to_string();

        let cc_cmd = make_cc_command_resolved_for_toolchain(
            &info.effective_native_toolchain,
            config,
            &info.cc_flags,
            &[],
            [input_file.display().to_string()],
            &intermediate_dir,
            Some(&output_file.display().to_string()),
            self.opt
                .backend
                .compiler_paths()
                .expect("native lowering requires compiler paths"),
        );

        let mut extra_inputs = Vec::with_capacity(1 + package.c_stub_header_files.len());
        extra_inputs.push(input_file.clone());
        extra_inputs.extend(package.c_stub_header_files.iter().cloned());

        BuildCommand::c_compile(&info.effective_native_toolchain, cc_cmd, extra_inputs)
    }

    #[instrument(level = Level::DEBUG, skip(self, artifacts, info))]
    pub(super) fn lower_archive_or_link_c_stubs(
        &mut self,
        artifacts: &ActionArtifacts,
        target: PackageId,
        info: &BuildCStubsInfo,
    ) -> BuildCommand {
        if !self.opt.backend.target_backend().is_native() {
            unreachable!("C stubs are only lowered for C or LLVM backends")
        }

        let object_files = artifacts.dependency_paths_matching(|artifact| {
            matches!(artifact, ArtifactKey::CStubObject { .. })
        });

        let output = artifacts.single_output_path_matching(|artifact| {
            matches!(
                artifact,
                ArtifactKey::CStubLibrary { package }
                    if *package == target
            )
        });
        self.lower_archive_c_stubs(info, &object_files, output)
    }

    fn lower_archive_c_stubs(
        &mut self,
        info: &BuildCStubsInfo,
        object_files: &[PathBuf],
        archive: PathBuf,
    ) -> BuildCommand {
        let config = ArchiverConfigBuilder::default()
            .archive_moonbitrun(false)
            .build()
            .expect("Failed to build archiver configuration");
        BuildCommand::archive(
            &info.effective_native_toolchain,
            config,
            object_files,
            &archive,
            self.opt
                .backend
                .compiler_paths()
                .expect("native lowering requires compiler paths"),
        )
    }

    #[instrument(level = Level::DEBUG, skip(self, artifacts, info))]
    pub(super) fn lower_make_exe(
        &mut self,
        artifacts: &ActionArtifacts,
        target: BuildTarget,
        info: &MakeExecutableInfo,
    ) -> BuildCommand {
        debug_assert!({
            let expected_c_stub = |artifact: &ArtifactKey, expected_package: Option<PackageId>| {
                let package = match artifact {
                    ArtifactKey::CStubLibrary { package } => Some(package),
                    _ => None,
                };
                package.is_some_and(|package| {
                    expected_package.is_none_or(|expected| *package == expected)
                })
            };
            let planned_c_stub_count = artifacts
                .dependency_paths_matching(|artifact| expected_c_stub(artifact, None))
                .len();
            planned_c_stub_count == info.link_c_stubs.len()
                && info.link_c_stubs.iter().all(|package| {
                    !artifacts
                        .dependency_paths_matching(|artifact| {
                            expected_c_stub(artifact, Some(*package))
                        })
                        .is_empty()
                })
        });

        match &self.opt.backend {
            BackendConfig::Wasm { .. } | BackendConfig::WasmGc { .. } | BackendConfig::Js => {
                unreachable!("non-native plans do not contain MakeExecutable actions")
            }
            BackendConfig::Native { .. } => match self.plan.backend_plan().native_mode() {
                NativeBackendMode::DirectObject(_) => {
                    self.lower_link_new_native_exe(artifacts, target, info)
                }
                NativeBackendMode::GeneratedC => {
                    self.lower_link_exe_regular(artifacts, target, info)
                }
            },
            BackendConfig::Llvm { .. } => self.lower_link_exe_regular(artifacts, target, info),
        }
    }

    /// Compile a target's generated C into its payload object, e.g.
    /// for package `lib` in a release build with a GCC-like compiler:
    ///
    /// ```text
    /// cc -o _build/native/release/build/lib/__moonbit_link_core__/lib.o \
    ///    -I$MOON_HOME/include -c -fwrapv -fno-strict-aliasing -O2 \
    ///    -DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_MIMALLOC \
    ///    _build/native/release/build/lib/lib.c
    /// ```
    #[instrument(level = Level::DEBUG, skip(self, artifacts, info))]
    pub(super) fn lower_compile_generated_c(
        &mut self,
        artifacts: &ActionArtifacts,
        target: BuildTarget,
        info: &CompileGeneratedCInfo,
    ) -> BuildCommand {
        let source = artifacts.single_dependency_path_matching(|artifact| {
            matches!(
                artifact,
                ArtifactKey::LinkedCore { package, target_kind }
                    if *package == target.package && *target_kind == target.kind
            )
        });
        let output = artifacts.single_output_path();

        // Compile with the settings an executable link would use for the
        // same generated C, so the object matches the runtime it links with.
        let config = CCConfigBuilder::default()
            .no_sys_header(true)
            .output_ty(CCOutputType::Object)
            .opt_level(info.c_opt_level)
            .debug_info(info.c_debug_info)
            .link_moonbitrun(true)
            .define_use_shared_runtime_macro(false)
            .native_allocator(info.native_allocator)
            .collect_ref_cycle(self.opt.backend.collect_ref_cycle())
            .build()
            .expect("Failed to build CC configuration for linked core object");

        let pkg_dir = self
            .artifact_paths
            .target_layout()
            .package_dir(
                &self.get_package(target).fqn,
                self.opt.backend.target_backend(),
            )
            .display()
            .to_string();

        let cc_cmd = make_cc_command_resolved_for_toolchain(
            &info.effective_native_toolchain,
            config,
            &info.c_flags,
            &[],
            [source.display().to_string()],
            &pkg_dir,
            Some(&output.display().to_string()),
            self.opt
                .backend
                .compiler_paths()
                .expect("native lowering requires compiler paths"),
        );

        BuildCommand::c_compile(&info.effective_native_toolchain, cc_cmd, vec![])
    }

    /// Archive a foreign library's payload, C stub, and runtime objects, e.g.
    /// for package `lib` importing `dep`, with GNU `ar` in a release build:
    ///
    /// ```text
    /// moon tool archive --output _build/native/release/build/lib/lib.a -- \
    ///   ar -r -c -s _build/native/release/build/lib/lib.a \
    ///    $MOON_HOME/lib/libmoonbitrun.o \
    ///    _build/native/release/build/lib/__moonbit_link_core__/lib.o \
    ///    _build/native/release/build/lib/lib_stub.o \
    ///    _build/native/release/build/dep/dep_stub.o \
    ///    _build/native/release/build/runtime-runtime.o ... \
    ///    $MOON_HOME/lib/simdutf.o
    /// ```
    ///
    /// Archivers that recreate the archive run directly with the same members,
    /// such as `libtool -static -o .../lib.a` on macOS or
    /// `lib /nologo /Out:...\lib.lib` with MSVC. Windows ships no
    /// `libmoonbitrun.o`.
    #[instrument(level = Level::DEBUG, skip(self, artifacts, info, runtime))]
    pub(super) fn lower_archive_foreign_library(
        &mut self,
        artifacts: &ActionArtifacts,
        info: &ForeignLibraryInfo,
        runtime: &BuildRuntimeInfo,
    ) -> BuildCommand {
        let archive = artifacts.single_output_path();

        // The MoonBit payload comes first, followed by the C stubs that may
        // reference runtime symbols, and then the runtime itself.
        let mut members = artifacts.dependency_paths_matching(|artifact| {
            matches!(
                artifact,
                ArtifactKey::LinkedCore { .. } | ArtifactKey::GeneratedCObject { .. }
            )
        });
        for package in &info.c_stub_packages {
            members.extend(artifacts.dependency_paths_matching(|artifact| {
                matches!(artifact, ArtifactKey::CStubObject { package: actual, .. } if actual == package)
            }));
        }
        members.extend(artifacts.dependency_paths_matching(|artifact| {
            matches!(artifact, ArtifactKey::RuntimeObject { .. })
        }));
        members.extend(runtime.simdutf_objects.iter().cloned());

        let config = ArchiverConfigBuilder::default()
            .archive_moonbitrun(true)
            .native_allocator(info.native_allocator)
            .build()
            .expect("Failed to build archiver configuration");
        let mut command = BuildCommand::archive(
            &info.effective_native_toolchain,
            config,
            &members,
            &archive,
            self.opt
                .backend
                .compiler_paths()
                .expect("native lowering requires compiler paths"),
        );
        command
            .extra_inputs
            .extend(runtime.simdutf_objects.iter().cloned());
        command
    }

    pub(super) fn lower_generate_dsym(
        &self,
        artifacts: &ActionArtifacts,
        target: BuildTarget,
        dsymutil: &Path,
    ) -> BuildCommand {
        let executable = artifacts.single_dependency_path_matching(|artifact| {
            matches!(
                artifact,
                ArtifactKey::Executable { package, target_kind }
                    if *package == target.package && *target_kind == target.kind
            )
        });
        BuildCommand {
            extra_inputs: vec![],
            commandline: vec![
                dsymutil.display().to_string(),
                executable.display().to_string(),
            ]
            .into(),
        }
    }

    fn native_executable_dependency_paths(
        &self,
        artifacts: &ActionArtifacts,
        info: &MakeExecutableInfo,
    ) -> Vec<PathBuf> {
        let mut sources = Vec::new();

        // Runtime is a static archive for regular builds, so place it after the
        // linked core and C stub archives that may introduce runtime symbols.
        sources.extend(artifacts.dependency_paths_matching(|artifact| {
            matches!(
                artifact,
                ArtifactKey::LinkedCore { .. } | ArtifactKey::GeneratedCObject { .. }
            )
        }));

        for package in &info.link_c_stubs {
            sources.extend(artifacts.dependency_paths_matching(|artifact| {
                matches!(artifact, ArtifactKey::CStubLibrary { package: actual } if actual == package)
            }));
        }

        sources.extend(
            artifacts.dependency_paths_matching(|artifact| {
                matches!(artifact, ArtifactKey::RuntimeLibrary)
            }),
        );

        sources
    }

    fn lower_link_exe_regular(
        &mut self,
        artifacts: &ActionArtifacts,
        target: BuildTarget,
        info: &MakeExecutableInfo,
    ) -> BuildCommand {
        // The generated-C payload is already compiled. Preserve the historical
        // driver flags here because they can also affect linking (e.g. LTO).

        let sources = self.native_executable_dependency_paths(artifacts, info);

        let config = LinkerConfigBuilder::<&Path>::default()
            .driver_flags(info.c_flags.clone())
            .opt_level(Some(info.c_opt_level))
            .debug_info(info.c_debug_info)
            .link_moonbitrun(true)
            .link_libbacktrace(true)
            .native_allocator(info.native_allocator)
            .build()
            .expect("Failed to build linker configuration for executable");

        let dest = artifacts.single_output_path().display().to_string();

        // This directory is used for MSVC to place intermediate files.
        // Each package should use their own to minimize conflicts.
        let pkg_dir = self
            .artifact_paths
            .target_layout()
            .package_dir(
                &self.get_package(target).fqn,
                self.opt.backend.target_backend(),
            )
            .display()
            .to_string();

        let cc_cmd = make_linker_command_resolved(
            info.effective_native_toolchain.cc().clone(),
            config,
            &info.link_args,
            &sources
                .iter()
                .map(|x| x.display().to_string())
                .collect::<Vec<_>>(),
            &pkg_dir,
            &dest,
            &self
                .opt
                .backend
                .compiler_paths()
                .expect("native lowering requires compiler paths")
                .lib_path,
        );

        BuildCommand {
            extra_inputs: vec![],
            commandline: cc_cmd.into(),
        }
        .with_msvc_env(&info.effective_native_toolchain)
    }

    fn lower_link_new_native_exe(
        &mut self,
        artifacts: &ActionArtifacts,
        target: BuildTarget,
        info: &MakeExecutableInfo,
    ) -> BuildCommand {
        let compiler_paths = self
            .opt
            .backend
            .compiler_paths()
            .expect("native lowering requires compiler paths");
        let sources = self.native_executable_dependency_paths(artifacts, info);
        let cc = info.effective_native_toolchain.cc().clone();

        let dest = artifacts.single_output_path().display().to_string();

        let pkg_dir = self
            .artifact_paths
            .target_layout()
            .package_dir(
                &self.get_package(target).fqn,
                self.opt.backend.target_backend(),
            )
            .display()
            .to_string();

        let config = LinkerConfigBuilder::<&Path>::default()
            .link_moonbitrun(true)
            .link_libbacktrace(true)
            .output_ty(CCOutputType::Executable)
            .native_allocator(info.native_allocator)
            .build()
            .expect("Failed to build LinkerConfig for new native executable");

        let source_args = sources
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        let commandline = if info.effective_native_toolchain.uses_msvc_cli() {
            compiler::msvc::link_executable_command(
                &info.effective_native_toolchain,
                &source_args,
                &info.link_args,
                &dest,
                &compiler_paths.lib_path,
            )
            .into()
        } else {
            let linker_cmd = make_linker_command_resolved(
                cc,
                config,
                &info.link_args,
                &source_args,
                &pkg_dir,
                &dest,
                &compiler_paths.lib_path,
            );
            linker_cmd.into()
        };

        BuildCommand {
            extra_inputs: vec![],
            commandline,
        }
        .with_msvc_env(&info.effective_native_toolchain)
    }

    #[instrument(level = Level::DEBUG, skip(self, artifacts))]
    pub(super) fn lower_parse_mbti(
        &mut self,
        artifacts: &ActionArtifacts,
        pid: PackageId,
        mbti_path: &Path,
    ) -> Result<BuildCommand, LoweringError> {
        let pkg = self.packages.get_package(pid);

        // The virtual package interface is emitted as the `.mi` of the source target
        let target = pid.build_target(TargetKind::Source);
        let mi_out = self.artifact_paths.mi_of_build_target(
            self.packages,
            &target,
            self.opt.backend.target_backend(),
        );

        // Resolve interface dependencies from the dep graph (path:alias pairs)
        let mi_inputs = self.mi_inputs_of(artifacts, target);

        // Construct `moonc build-interface` command
        let cmd = compiler::MooncBuildInterface::new(
            mbti_path,
            mi_out.as_path(),
            &mi_inputs.deps,
            compiler::CompiledPackageName::new(&pkg.fqn, TargetKind::Source),
            &pkg.root_path,
        );

        let commandline = moonc_command::lower(cmd.build_command(&*BINARIES.moonc), &mi_out)?;

        // Track the user-written `.mbti` contract as an explicit input
        let mut extra_inputs = vec![mbti_path.to_path_buf()];
        extra_inputs.extend(mi_inputs.installed_stdlib);

        Ok(BuildCommand {
            extra_inputs,
            commandline,
        })
    }

    #[instrument(level = Level::DEBUG, skip(self, artifacts))]
    pub(super) fn mi_inputs_of(
        &self,
        artifacts: &ActionArtifacts,
        target: BuildTarget,
    ) -> MiInputs<'a> {
        let mut stdlib_deps = Vec::new();
        // The package graph retains compiler-facing aliases and ordering. The
        // selected artifact requirement supplies each dependency's path.
        let mut deps: Vec<MiDependency<'a>> = self
            .rel
            .dep_graph
            .edges_directed(target, Direction::Outgoing)
            .map(|(_, dep, w)| {
                let in_file = if self.packages.is_stdlib_package(dep.package) {
                    // Installed stdlib interfaces are selected by the known
                    // toolchain layout, outside this Build Plan's providers.
                    stdlib_deps.push(dep);
                    self.artifact_paths.mi_of_build_target(
                        self.packages,
                        &dep,
                        self.opt.backend.target_backend(),
                    )
                } else {
                    artifacts.single_dependency_path_matching(|artifact| {
                        matches!(
                            artifact,
                            ArtifactKey::CheckMi {
                                package,
                                target_kind,
                            }
                                | ArtifactKey::BuildMi {
                                    package,
                                    target_kind,
                                }
                                | ArtifactKey::ProofMi {
                                    package,
                                    target_kind,
                                }
                                if *package == dep.package && *target_kind == dep.kind
                        ) || matches!(
                            artifact,
                            ArtifactKey::VirtualContractMi { package }
                                if *package == dep.package
                        )
                    })
                };
                MiDependency::new(in_file, w.short_alias.as_str(), w.import_all)
            })
            .collect::<Vec<_>>();
        deps.sort_by(|x, y| x.alias.cmp(&y.alias));
        MiInputs {
            deps,
            installed_stdlib: self.installed_stdlib_closure(stdlib_deps),
        }
    }

    /// Installed stdlib interfaces that `moonc` may read when importing `roots`.
    ///
    /// Besides each `-i` file, `moonc` loads the interfaces of indirect
    /// dependencies through the paths listed in `all_pkgs.json`. Local
    /// interfaces are covered by their producers, but installed stdlib
    /// interfaces have none, so the whole reachable stdlib closure is observed.
    fn installed_stdlib_closure(&self, roots: Vec<BuildTarget>) -> Vec<PathBuf> {
        let mut visited = HashSet::new();
        let mut pending = roots;
        let mut paths = BTreeSet::new();
        while let Some(current) = pending.pop() {
            if !visited.insert(current) {
                continue;
            }
            paths.insert(self.artifact_paths.mi_of_build_target(
                self.packages,
                &current,
                self.opt.backend.target_backend(),
            ));
            pending.extend(
                self.rel
                    .dep_graph
                    .neighbors_directed(current, Direction::Outgoing)
                    .filter(|dep| self.packages.is_stdlib_package(dep.package)),
            );
        }
        paths.into_iter().collect()
    }
}

/// Interface dependencies of one compiler action.
pub(super) struct MiInputs<'a> {
    /// `-i` arguments, sorted by alias.
    pub(super) deps: Vec<MiDependency<'a>>,
    /// Installed stdlib interfaces reachable from `deps`. No action in this
    /// plan produces them, so the action observes them as explicit file inputs.
    pub(super) installed_stdlib: Vec<PathBuf>,
}
