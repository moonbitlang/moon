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

//! Lowering implementation for build actions.

use std::{
    collections::{BTreeSet, HashSet},
    path::{Path, PathBuf},
};

use moonutil::{
    common::RunMode,
    compiler_flags::{
        ArchiverConfigBuilder, CC, CCConfigBuilder, LinkerConfigBuilder, OptLevel as CCOptLevel,
        OutputType as CCOutputType, make_archiver_command_resolved,
        make_cc_command_resolved_for_toolchain, make_cc_command_resolved_with_link_flags,
        make_linker_command_resolved,
    },
    cond_expr::OptLevel,
    mooncakes::{CORE_MODULE, ModuleId},
    package::JsFormat,
    toolchain::BINARIES,
};
use petgraph::Direction;
use tracing::{Level, instrument};

use crate::{
    build_action_plan::BuildProduct,
    build_lower::{
        CExecutableRealization, CStubLibraryRealization, SelectedBackend, WarningCondition,
        compiler::{
            BuildCommonConfig, BuildCommonInput, CmdlineAbstraction, ErrorFormat, JsConfig,
            MiDependency, PackageSource, WasmConfig,
        },
    },
    build_plan::{BuildCStubsInfo, BuildTargetInfo, LinkCoreInfo, MakeExecutableInfo},
    discover::DiscoveredPackage,
    model::{BuildTarget, NativeTarget, OperatingSystem, PackageId, RunBackend, TargetKind},
    pkg_name::{PackageFQN, PackagePath},
    special_cases::{is_self_coverage_lib, should_skip_coverage},
    target_layout::proof_artifact_stem,
};

use super::{
    BuildCommand, Commandline, compiler,
    context::{ActionProducts, LoweringContext},
};

fn commandline_with_dsymutil(cmd: &[String], dest: &str) -> Commandline {
    let cmd_str = moonutil::shlex::join_unix(cmd.iter().map(|x| x.as_str()));
    let dsymutil_args = ["dsymutil", dest];
    let dsymutil_cmd_str = moonutil::shlex::join_unix(dsymutil_args.iter().copied());
    Commandline::Verbatim(format!("{cmd_str} && {dsymutil_cmd_str}"))
}

fn should_run_new_native_dsymutil(
    native_target: Option<NativeTarget>,
    debug_symbols: bool,
    cc: &CC,
) -> bool {
    native_target == Some(NativeTarget::Aarch64AppleDarwin)
        && debug_symbols
        && cc.targets_apple_darwin()
}

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
        pkg: &DiscoveredPackage,
        info: &'a BuildTargetInfo,
        is_main: bool,
    ) -> BuildCommonConfig<'a> {
        // Standard library settings
        let stdlib_core_file =
            self.opt.stdlib_path.as_ref().map(|x| {
                moonutil::toolchain::core_bundle_in(x, self.opt.target_backend.into()).into()
            });

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

        // Compute -check-mi and virtual implementation mapping when requested
        let mut virtual_implementation = None;
        let mut check_mi = None;
        let no_mi = info.no_mi();

        if let Some(v_target) = info.check_mi_against {
            // The target to check against is always the Source target of the virtual package
            let mi_path = self.artifact_paths.mi_of_build_target(
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

            // Implementation package will generate a dummy mi file so that it
            // won't be rebuilt every time
        }

        BuildCommonConfig {
            stdlib_core_file,
            error_format,
            deny_warn,
            warn_config,
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

    fn prove_mi_inputs_of(&self, target: BuildTarget) -> Vec<MiDependency<'a>> {
        let mut deps: Vec<MiDependency<'a>> = self
            .rel
            .dep_graph
            .edges_directed(target, Direction::Outgoing)
            .map(|(_, dep, w)| {
                // `moonc prove` still expects stdlib interfaces as explicit `-i`
                // inputs, but those come from the injected toolchain location
                // (for example `$MOON_HOME/lib/core/_build/wasm-gc/.../prelude.mi`)
                // rather than from proof emission. Non-stdlib proof deps instead
                // use the emitted proof interface under `_build/verif`, for
                // example `_build/verif/pkg_<stem>.mi`.
                let in_file = if self.opt.stdlib_path.is_some()
                    && self.packages.is_stdlib_package(dep.package)
                {
                    self.artifact_paths.mi_of_build_target(
                        self.packages,
                        &dep,
                        self.opt.target_backend.into(),
                    )
                } else {
                    self.artifact_paths
                        .target_layout()
                        .emit_proof_mi_path(self.packages, &dep)
                };
                MiDependency::new(in_file, &w.short_alias)
            })
            .collect();
        deps.sort_by(|x, y| x.alias.cmp(&y.alias));
        deps
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
            .filter(|(_, dep, _)| {
                !(self.opt.stdlib_path.is_some() && self.packages.is_stdlib_package(dep.package))
            })
            .map(|(_, dep, _)| {
                let pkg = self.packages.get_package(dep.package);
                compiler::DepProof::new(pkg.fqn.to_string(), proof_artifact_stem(&pkg.fqn))
            })
            .collect::<Vec<_>>();
        deps.sort_by(|x, y| x.package.cmp(&y.package));
        deps
    }

    fn proof_loadpaths_of(&self, target: BuildTarget) -> Vec<PathBuf> {
        let mut visited = HashSet::new();
        let mut pending = vec![target];
        let mut loadpaths = BTreeSet::new();

        if let Some(prelude_proof) = self.stdlib_prelude_proof_loadpath() {
            loadpaths.insert(prelude_proof);
        }

        while let Some(current) = pending.pop() {
            if !visited.insert(current) {
                continue;
            }

            for dep in self
                .rel
                .dep_graph
                .neighbors_directed(current, Direction::Outgoing)
            {
                if self.opt.stdlib_path.is_some() && self.packages.is_stdlib_package(dep.package) {
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

    fn stdlib_prelude_proof_loadpath(&self) -> Option<PathBuf> {
        let prelude_proof = self
            .opt
            .stdlib_path
            .as_ref()
            .and_then(|stdlib_root| stdlib_root.parent())
            .map(|lib_root| lib_root.join(moonutil::common::PRELUDE_PROOF_DIR))?;

        prelude_proof.is_dir().then_some(prelude_proof)
    }

    #[instrument(level = Level::DEBUG, skip(self, products, info))]
    pub(super) fn lower_check(
        &self,
        products: &ActionProducts,
        target: BuildTarget,
        info: &BuildTargetInfo,
    ) -> BuildCommand {
        let package = self.get_package(target);
        let module = self.packages.module_info(package.module);

        let mi_output = products
            .optional_single_output_path_matching(|product| {
                matches!(
                    product,
                    BuildProduct::PackageInterface {
                        target: product_target,
                        ..
                    } if *product_target == target
                )
            })
            .unwrap_or_else(|| {
                if info.check_mi_against.is_some() {
                    self.artifact_paths
                        .mi_of_build_target_impl_virtual(
                            self.packages,
                            &target,
                            self.opt.target_backend.into(),
                        )
                        .into_path()
                } else {
                    unreachable!("regular Check actions should have one package interface product")
                }
            });
        let mi_inputs = self.mi_inputs_of(target);

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
        let backend = self.opt.target_backend.into();
        let cmd = compiler::MooncCheck {
            required: BuildCommonInput::new(
                &files_vec,
                &info.doctest_files,
                &mi_inputs,
                compiler::CompiledPackageName::new(&package.fqn, target.kind),
                &package.root_path,
                self.artifact_paths
                    .target_layout()
                    .all_pkgs_of_build_target(backend),
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
        if !package.is_single_file {
            extra_inputs.push(package.config_path());
        }

        // Also track any -check-mi file used by this command (virtual checks/impl)
        self.extend_extra_inputs(&cmd.defaults, &mut extra_inputs);

        BuildCommand {
            extra_inputs,
            commandline: cmd.build_command(&*BINARIES.moonc).into(),
        }
    }

    #[instrument(level = Level::DEBUG, skip(self, products, info))]
    pub(super) fn lower_emit_proof(
        &self,
        products: &ActionProducts,
        target: BuildTarget,
        info: &BuildTargetInfo,
    ) -> BuildCommand {
        let package = self.get_package(target);
        let module = self.packages.module_info(package.module);
        let mi_inputs = self.prove_mi_inputs_of(target);

        let files_vec = self.compiler_source_files(info);

        let backend = self.opt.target_backend.into();
        let whyml_output = products.single_output_path_matching(|product| {
            matches!(
                product,
                BuildProduct::ProofWhyml {
                    target: product_target,
                    ..
                } if *product_target == target
            )
        });
        let dep_proofs = self.dep_proofs_of(target);
        let cmd = compiler::MooncProve {
            required: BuildCommonInput::new(
                &files_vec,
                &info.doctest_files,
                &mi_inputs,
                compiler::CompiledPackageName::new(&package.fqn, target.kind),
                &package.root_path,
                self.artifact_paths
                    .target_layout()
                    .all_pkgs_of_build_target(backend),
                backend,
                target.kind,
            ),
            defaults: self.set_build_commons(package, info, package.raw.is_main),
            whyml_out: whyml_output.clone().into(),
            proof_report_out: None,
            why3_config: None,
            why3_loadpaths: Vec::new(),
            dep_proofs,
            emit_only: true,
            single_file: package.is_single_file,
            extra_flags: module.compile_flags.as_deref().unwrap_or_default(),
        };

        let mut extra_inputs = files_vec.clone();
        extra_inputs.extend(info.doctest_files.clone());
        if !package.is_single_file {
            extra_inputs.push(package.config_path());
        }
        self.extend_extra_inputs(&cmd.defaults, &mut extra_inputs);

        BuildCommand {
            extra_inputs,
            commandline: cmd.build_command(&*BINARIES.moonc).into(),
        }
    }

    #[instrument(level = Level::DEBUG, skip(self, products, info))]
    pub(super) fn lower_prove(
        &self,
        products: &ActionProducts,
        target: BuildTarget,
        info: &BuildTargetInfo,
    ) -> BuildCommand {
        let package = self.get_package(target);
        let module = self.packages.module_info(package.module);
        let mi_inputs = self.prove_mi_inputs_of(target);

        let files_vec = self.compiler_source_files(info);

        let backend = self.opt.target_backend.into();
        let why3_config = info
            .why3_config
            .clone()
            .unwrap_or_else(|| self.artifact_paths.target_layout().why3_config_path());
        let whyml_output = products.single_output_path_matching(|product| {
            matches!(
                product,
                BuildProduct::ProofWhyml {
                    target: product_target,
                    ..
                } if *product_target == target
            )
        });
        let proof_report_output = products.single_output_path_matching(|product| {
            matches!(
                product,
                BuildProduct::ProofReport {
                    target: product_target,
                    ..
                } if *product_target == target
            )
        });
        let dep_proofs = self.dep_proofs_of(target);
        // Why3 needs every reachable non-stdlib proof directory on its loadpath
        // once emitted proof artifacts live alongside package-local verification
        // outputs under `_build/verif/<pkg>/...`.
        let why3_loadpaths = self.proof_loadpaths_of(target);
        let cmd = compiler::MooncProve {
            required: BuildCommonInput::new(
                &files_vec,
                &info.doctest_files,
                &mi_inputs,
                compiler::CompiledPackageName::new(&package.fqn, target.kind),
                &package.root_path,
                self.artifact_paths
                    .target_layout()
                    .all_pkgs_of_build_target(backend),
                backend,
                target.kind,
            ),
            defaults: self.set_build_commons(package, info, package.raw.is_main),
            whyml_out: whyml_output.clone().into(),
            proof_report_out: Some(proof_report_output.clone().into()),
            why3_config: Some(why3_config.clone().into()),
            why3_loadpaths: why3_loadpaths.clone(),
            dep_proofs,
            emit_only: false,
            single_file: package.is_single_file,
            extra_flags: module.compile_flags.as_deref().unwrap_or_default(),
        };

        let mut extra_inputs = files_vec.clone();
        extra_inputs.extend(info.doctest_files.clone());
        extra_inputs.push(why3_config);
        extra_inputs.extend(why3_loadpaths);
        if !package.is_single_file {
            extra_inputs.push(package.config_path());
        }
        self.extend_extra_inputs(&cmd.defaults, &mut extra_inputs);

        BuildCommand {
            extra_inputs,
            commandline: cmd.build_command(&*BINARIES.moonc).into(),
        }
    }

    #[instrument(level = Level::DEBUG, skip(self, products, info))]
    pub(super) fn lower_build_mbt(
        &self,
        products: &ActionProducts,
        target: BuildTarget,
        info: &BuildTargetInfo,
    ) -> BuildCommand {
        let package = self.get_package(target);
        let module = self.packages.module_info(package.module);

        let core_output = products.single_output_path_matching(|product| {
            matches!(
                product,
                BuildProduct::PackageCoreIr {
                    target: product_target,
                    ..
                } if *product_target == target
            )
        });
        let mi_output = products
            .optional_single_output_path_matching(|product| {
                matches!(
                    product,
                    BuildProduct::PackageInterface {
                        target: product_target,
                        ..
                    } if *product_target == target
                )
            })
            .unwrap_or_else(|| {
                self.artifact_paths.mi_of_build_target(
                    self.packages,
                    &target,
                    self.opt.target_backend.into(),
                )
            });

        let mi_inputs = self.mi_inputs_of(target);

        let mut files = self.compiler_source_files(info);
        match target.kind {
            TargetKind::Source | TargetKind::SubPackage => {}
            TargetKind::WhiteboxTest | TargetKind::BlackboxTest | TargetKind::InlineTest => {
                let test_driver = products.single_dependency_path_matching(|product| {
                    matches!(
                        product,
                        BuildProduct::GeneratedTestDriver {
                            target: product_target,
                            ..
                        } if *product_target == target
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
        let backend = self.opt.target_backend.into();
        let mut cmd = compiler::MooncBuildPackage {
            required: BuildCommonInput::new(
                &files,
                &info.doctest_files,
                &mi_inputs,
                compiler::CompiledPackageName::new(&package.fqn, target.kind),
                &package.root_path,
                self.artifact_paths
                    .target_layout()
                    .all_pkgs_of_build_target(backend),
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
            self.get_coverage_flags(target, package, &package.fqn, true);
        cmd.defaults.no_mi |= target.kind.is_test() | (cmd.defaults.check_mi.is_some());

        // Include doctest-only files as inputs to track dependency correctly
        // Note: This is the *extra* inputs, trivial dependencies are already
        // tracked via the build graph.
        let mut extra_inputs = self.compiler_source_files(info);
        extra_inputs.extend(info.doctest_files.clone());
        if !package.is_single_file {
            extra_inputs.push(package.config_path());
        }

        self.extend_extra_inputs(&cmd.defaults, &mut extra_inputs);

        BuildCommand {
            commandline: cmd.build_command(&*BINARIES.moonc).into(),
            extra_inputs,
        }
    }

    #[instrument(level = Level::DEBUG, skip(self, products, info))]
    pub(super) fn lower_link_core(
        &mut self,
        products: &ActionProducts,
        target: BuildTarget,
        info: &LinkCoreInfo,
        make_executable_info: Option<&MakeExecutableInfo>,
    ) -> BuildCommand {
        #[cfg(not(target_os = "windows"))]
        let _ = make_executable_info;

        let package = self.get_package(target);
        let module = self.packages.module_info(package.module);

        let mut core_input_files = Vec::new();
        // Add core for the standard library
        if let Some(stdlib) = &self.opt.stdlib_path {
            // The two stdlib core files must be linked in the correct order,
            // in order to get the correct order of initialization.
            if !info.abort_overridden {
                core_input_files.push(moonutil::toolchain::abort_core_in(
                    stdlib,
                    self.opt.target_backend.into(),
                ));
            }
            core_input_files.push(moonutil::toolchain::core_core_in(
                stdlib,
                self.opt.target_backend.into(),
            ));
        }
        // Linked core targets
        for target in &info.linked_order {
            let core_path = products.single_dependency_path_matching(|product| {
                matches!(
                    product,
                    BuildProduct::PackageCoreIr {
                        target: product_target,
                        ..
                    } if product_target == target
                )
            });
            core_input_files.push(core_path);
        }

        let out_file = products.single_output_path_matching(|product| {
            matches!(
                product,
                BuildProduct::LinkedCore {
                    target: product_target,
                    ..
                } if *product_target == target
            )
        });

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
            pkg_config_path: config_path.clone().into(),
            package_sources: &package_sources,
            stdlib_core_source: None,
            target_backend: self.opt.target_backend.into(),
            native_target: self.opt.native_mode.direct_target(),
            flags: self.set_flags(),
            test_mode: target.kind.is_test(),
            wasm_config: self.get_wasm_config(target, package),
            js_config: self.get_js_config(target, package),
            exports: package.exported_functions(self.opt.target_backend.into()),
            extra_link_opts: module.link_flags.as_deref().unwrap_or_default(),
            #[cfg(target_os = "windows")]
            native_toolchain_is_msvc: make_executable_info
                .is_some_and(|info| info.effective_native_toolchain.cc().is_msvc()),
        };

        // Ensure n2 sees stdlib core bundle changes as inputs
        let mut extra_inputs = Vec::new();
        if let Some(stdlib) = &self.opt.stdlib_path {
            extra_inputs.push(moonutil::toolchain::abort_core_in(
                stdlib,
                self.opt.target_backend.into(),
            ));
            extra_inputs.push(moonutil::toolchain::core_core_in(
                stdlib,
                self.opt.target_backend.into(),
            ));
        }
        if !package.is_single_file {
            extra_inputs.push(config_path);
        }

        BuildCommand {
            extra_inputs,
            commandline: cmd.build_command(&*BINARIES.moonc).into(),
        }
    }

    fn get_wasm_config<'b>(
        &self,
        target: BuildTarget,
        pkg: &'b DiscoveredPackage,
    ) -> compiler::WasmConfig<'b> {
        let mut wasm_config = if self.opt.target_backend == RunBackend::Wasm
            && let Some(cfg) = pkg.raw.link.as_ref().and_then(|x| x.wasm.as_ref())
        {
            WasmConfig {
                export_memory_name: cfg.export_memory_name.as_deref().map(|x| x.into()),
                import_memory: cfg.import_memory.as_ref(),
                memory_limits: cfg.memory_limits.as_ref(),
                shared_memory: cfg.shared_memory,
                heap_start_address: cfg.heap_start_address,
                link_flags: cfg.flags.as_deref(),
                wasi: false,
            }
        } else if self.opt.target_backend == RunBackend::WasmGC
            && let Some(cfg) = pkg.raw.link.as_ref().and_then(|x| x.wasm_gc.as_ref())
        {
            WasmConfig {
                export_memory_name: cfg.export_memory_name.as_deref().map(|x| x.into()),
                import_memory: cfg.import_memory.as_ref(),
                memory_limits: cfg.memory_limits.as_ref(),
                shared_memory: cfg.shared_memory,
                heap_start_address: None,
                link_flags: cfg.flags.as_deref(),
                wasi: false,
            }
        } else {
            WasmConfig::default()
        };

        if self.should_link_wasi(target, pkg) {
            wasm_config.wasi = true;
        }

        wasm_config
    }

    fn should_link_wasi(&self, target: BuildTarget, pkg: &DiscoveredPackage) -> bool {
        if !self.opt.wasi_link || self.opt.target_backend != RunBackend::Wasm {
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

    #[instrument(level = Level::DEBUG, skip(self, products, info))]
    pub(super) fn lower_build_c_stub(
        &mut self,
        products: &ActionProducts,
        target: PackageId,
        index: u32,
        info: &BuildCStubsInfo,
    ) -> BuildCommand {
        if !self.opt.target_backend.is_native() {
            unreachable!("C stubs are only lowered for C or LLVM backends")
        }

        let package = self.packages.get_package(target);

        let input_file = &package.c_stub_files[index as usize];
        let output_file = products.single_output_path_matching(|product| {
            matches!(
                product,
                BuildProduct::CStubObject {
                    package: product_package,
                    index: product_index,
                    ..
                } if *product_package == target && *product_index == index
            )
        });

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
        let use_shared_runtime = self.opt.selected_backend.uses_shared_runtime();

        let config = CCConfigBuilder::default()
            .no_sys_header(true)
            .output_ty(CCOutputType::Object)
            .opt_level(opt_level)
            .debug_info(self.opt.debug_symbols)
            .link_moonbitrun(!use_shared_runtime)
            .define_use_shared_runtime_macro(use_shared_runtime)
            .msvc_static_runtime(self.opt.selected_backend.is_windows_msvc_direct())
            .build()
            .expect("Failed to build CC configuration for C stub");

        let intermediate_dir = self
            .artifact_paths
            .target_layout()
            .package_dir(&package.fqn, self.opt.target_backend.into())
            .display()
            .to_string();

        let cc_cmd = make_cc_command_resolved_for_toolchain(
            &info.effective_native_toolchain,
            config,
            &info.cc_flags,
            &[] as &[&str],
            [input_file.display().to_string()],
            &intermediate_dir,
            Some(&output_file.display().to_string()),
            self.opt.compiler_paths(),
        );

        BuildCommand {
            commandline: cc_cmd.into(),
            extra_inputs: vec![input_file.clone()],
        }
    }

    #[instrument(level = Level::DEBUG, skip(self, products, info))]
    pub(super) fn lower_archive_or_link_c_stubs(
        &mut self,
        products: &ActionProducts,
        target: PackageId,
        info: &BuildCStubsInfo,
    ) -> BuildCommand {
        if !self.opt.target_backend.is_native() {
            unreachable!("C stubs are only lowered for C or LLVM backends")
        }

        let object_files = products.dependency_paths_matching(|product| {
            matches!(product, BuildProduct::CStubObject { .. })
        });

        // There's two ways to handle this:
        // - When not using `tcc -run`, this creates an archive of the C stubs.
        // - When using `tcc -run`, this links the C stubs to an ELF .so, so tcc
        //   can load it at runtime.
        let output = products.single_output_path_matching(|product| {
            matches!(
                product,
                BuildProduct::CStubLibrary {
                    package: product_package,
                    ..
                } if *product_package == target
            )
        });

        match self.opt.selected_backend.c_stub_library_realization() {
            CStubLibraryRealization::SharedLibraryForTccRun => {
                self.lower_link_c_stubs(products, info, &object_files, output)
            }
            CStubLibraryRealization::StaticArchive => {
                self.lower_archive_c_stubs(info, &object_files, output)
            }
        }
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

        let cc = info.effective_native_toolchain.cc().clone();
        let archiver_cmd = make_archiver_command_resolved(
            cc,
            config,
            &object_files
                .iter()
                .map(|s| s.to_string_lossy())
                .collect::<Vec<_>>(),
            &archive.display().to_string(),
            self.opt.compiler_paths(),
        );

        BuildCommand {
            extra_inputs: vec![],
            commandline: archiver_cmd.into(),
        }
    }

    fn lower_link_c_stubs(
        &mut self,
        products: &ActionProducts,
        info: &BuildCStubsInfo,
        object_files: &[PathBuf],
        dylib_out: PathBuf,
    ) -> BuildCommand {
        let dest_dir = dylib_out
            .parent()
            .expect("c stub dylib should have a parent directory")
            .display()
            .to_string();

        // Track libruntime.{DYN_EXT} as a dependency but do not pass it as a direct linker src.
        // Legacy adds runtime into build inputs then links via -lruntime using link_shared_runtime.
        let runtime_dylib = products
            .single_dependency_path_matching(|product| matches!(product, BuildProduct::RuntimeLib));
        let runtime_parent = runtime_dylib
            .parent()
            .expect("runtime dylib should have a parent directory");

        // Use the effective toolchain (already resolved at planning time)
        let cc = info.effective_native_toolchain.cc().clone();

        // Build linker config: shared lib, no libmoonbitrun, and link shared runtime dir
        let lcfg = LinkerConfigBuilder::<&Path>::default()
            .link_moonbitrun(false) // this is only for tcc -run
            .link_libbacktrace(true)
            .output_ty(CCOutputType::SharedLib)
            .link_shared_runtime(Some(runtime_parent))
            .build()
            .expect("Failed to build LinkerConfig for C stub dylib");

        // Sources: only object files; runtime handled via link_shared_runtime (-lruntime + rpath)
        let sources: Vec<String> = object_files
            .iter()
            .map(|p| p.display().to_string())
            .collect();

        let cc_cmd = make_linker_command_resolved(
            cc,
            lcfg,
            &info.link_flags,
            &sources,
            &dest_dir,
            &dylib_out.display().to_string(),
            &self.opt.compiler_paths().lib_path,
        );

        // Note: Runtime input is tracked in build plan, so no need to add here.
        BuildCommand {
            extra_inputs: vec![],
            commandline: cc_cmd.into(),
        }
    }

    #[instrument(level = Level::DEBUG, skip(self, products, info))]
    pub(super) fn lower_make_exe(
        &mut self,
        products: &ActionProducts,
        target: BuildTarget,
        info: &MakeExecutableInfo,
    ) -> BuildCommand {
        debug_assert!({
            let planned_c_stub_count = products
                .dependency_paths_matching(|product| {
                    matches!(product, BuildProduct::CStubLibrary { .. })
                })
                .len();
            planned_c_stub_count == info.link_c_stubs.len()
                && info.link_c_stubs.iter().all(|package| {
                    !products
                        .dependency_paths_matching(|product| {
                            matches!(
                                product,
                                BuildProduct::CStubLibrary {
                                    package: actual,
                                    ..
                                } if actual == package
                            )
                        })
                        .is_empty()
                })
        });

        match &self.opt.selected_backend {
            SelectedBackend::Wasm { .. } | SelectedBackend::WasmGc { .. } | SelectedBackend::Js => {
                unreachable!("non-native make-executable actions are no-ops during lowering")
            }
            SelectedBackend::C(backend) => match backend.executable_realization() {
                CExecutableRealization::WriteTccRunResponseFile => {
                    let tcc_run = backend
                        .tcc_run()
                        .expect("tcc-run realization should carry tcc-run config");
                    let internal_tcc = tcc_run.internal_tcc().clone();
                    self.build_tcc_run_driver_command(products, info, internal_tcc)
                }
                CExecutableRealization::LinkDirectObject => {
                    self.lower_link_new_native_exe(products, target, info)
                }
                CExecutableRealization::CompileAndLinkGeneratedC => {
                    self.lower_build_exe_regular(products, target, info)
                }
            },
            SelectedBackend::Llvm => self.lower_build_exe_regular(products, target, info),
        }
    }

    fn native_executable_dependency_paths(
        &self,
        products: &ActionProducts,
        info: &MakeExecutableInfo,
        include_linked_core: bool,
    ) -> Vec<PathBuf> {
        let mut sources = Vec::new();

        // Preserve the legacy linker order: linked core, runtime, then C stubs.
        // Static library order can affect symbol resolution on Unix linkers.
        if include_linked_core {
            sources.extend(products.dependency_paths_matching(|product| {
                matches!(product, BuildProduct::LinkedCore { .. })
            }));
        }

        sources.extend(
            products
                .dependency_paths_matching(|product| matches!(product, BuildProduct::RuntimeLib)),
        );

        for package in &info.link_c_stubs {
            sources.extend(products.dependency_paths_matching(|product| {
                matches!(product, BuildProduct::CStubLibrary { package: actual, .. } if actual == package)
            }));
        }

        sources
    }

    fn lower_build_exe_regular(
        &mut self,
        products: &ActionProducts,
        target: BuildTarget,
        info: &MakeExecutableInfo,
    ) -> BuildCommand {
        let _package = self.get_package(target);

        // Two things needs to be done here:
        // - compile the program (if needed)
        // - link with runtime library & artifacts of other C stubs

        let mut sources = self.native_executable_dependency_paths(products, info, true);
        let cc = info.effective_native_toolchain.cc().clone();
        let simdutf_objects = if cc.can_use_simdutf() {
            self.opt
                .compiler_paths()
                .simdutf_object_paths()
                .map(|objects| objects.into_iter().collect::<Vec<_>>())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        sources.extend(simdutf_objects.iter().cloned());

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
            .link_libbacktrace(true)
            .define_use_shared_runtime_macro(false)
            .build()
            .expect("Failed to build CC configuration for executable");

        let dest = products.single_output_path().display().to_string();

        // This directory is used for MSVC to place intermediate files.
        // Each package should use their own to minimize conflicts.
        let pkg_dir = self
            .artifact_paths
            .target_layout()
            .package_dir(
                &self.get_package(target).fqn,
                self.opt.target_backend.into(),
            )
            .display()
            .to_string();

        let cc_cmd = make_cc_command_resolved_with_link_flags(
            cc,
            config,
            &info.c_flags,
            &info.link_flags,
            sources.iter().map(|x| x.display().to_string()),
            &pkg_dir,
            Some(&dest),
            self.opt.compiler_paths(),
        );

        // On macOS with LLVM backend and debug symbols, run dsymutil after linking
        // to generate the dSYM bundle for better debugging experience
        let commandline = if self.opt.target_backend == RunBackend::Llvm
            && self.opt.debug_symbols
            && self.opt.os() == OperatingSystem::MacOS
        {
            commandline_with_dsymutil(&cc_cmd, &dest)
        } else {
            cc_cmd.into()
        };

        BuildCommand {
            extra_inputs: simdutf_objects,
            commandline,
        }
    }

    fn lower_link_new_native_exe(
        &mut self,
        products: &ActionProducts,
        target: BuildTarget,
        info: &MakeExecutableInfo,
    ) -> BuildCommand {
        let mut sources = self.native_executable_dependency_paths(products, info, true);

        let cc = info.effective_native_toolchain.cc().clone();
        let simdutf_objects = if cc.can_use_simdutf() {
            self.opt
                .compiler_paths()
                .simdutf_object_paths()
                .map(|objects| objects.into_iter().collect::<Vec<_>>())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        sources.extend(simdutf_objects.iter().cloned());

        let dest = products.single_output_path().display().to_string();

        let pkg_dir = self
            .artifact_paths
            .target_layout()
            .package_dir(
                &self.get_package(target).fqn,
                self.opt.target_backend.into(),
            )
            .display()
            .to_string();

        let config = LinkerConfigBuilder::<&Path>::default()
            .link_moonbitrun(true)
            .link_libbacktrace(true)
            .output_ty(CCOutputType::Executable)
            .build()
            .expect("Failed to build LinkerConfig for new native executable");

        let source_args = sources
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        let run_dsymutil = should_run_new_native_dsymutil(
            self.opt.native_mode.direct_target(),
            self.opt.debug_symbols,
            &cc,
        );

        let commandline = if self.opt.selected_backend.is_windows_msvc_direct() {
            assert!(
                cc.is_msvc(),
                "Windows MSVC native backend requires an MSVC cl-compatible compiler driver; found {}",
                cc.cc_path
            );
            compiler::msvc::link_executable_command(
                &info.effective_native_toolchain,
                &source_args,
                &info.link_flags,
                &dest,
                &self.opt.compiler_paths().lib_path,
            )
            .into()
        } else {
            let linker_cmd = make_linker_command_resolved(
                cc,
                config,
                &info.link_flags,
                &source_args,
                &pkg_dir,
                &dest,
                &self.opt.compiler_paths().lib_path,
            );
            if run_dsymutil {
                commandline_with_dsymutil(&linker_cmd, &dest)
            } else {
                linker_cmd.into()
            }
        };

        BuildCommand {
            extra_inputs: simdutf_objects,
            commandline,
        }
    }

    /// Build the command for `tcc -run` to execute when running, as well as
    /// putting that into a response file.
    fn build_tcc_run_driver_command(
        &self,
        products: &ActionProducts,
        info: &MakeExecutableInfo,
        cc: CC,
    ) -> BuildCommand {
        let sources = self.native_executable_dependency_paths(products, info, false);

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

        let mut cmdline = make_cc_command_resolved_with_link_flags(
            cc,
            cfg,
            &[] as &[&str], // no user flags
            &info.link_flags,
            sources.iter().map(|x| x.to_string_lossy().into_owned()),
            "", // TCC is not MSVC, no need to set special dest dir
            None,
            self.opt.compiler_paths(),
        );

        // The C file from moonc link-core
        cmdline.push("-run".to_string());
        let c_file = products.single_dependency_path_matching(|product| {
            matches!(product, BuildProduct::LinkedCore { .. })
        });
        cmdline.push(c_file.display().to_string());

        // Note: at this point, we have our TCC command.
        // However, this command should be executed when the user runs the final
        // executable, not in this build graph. Thus, we need to put them into
        // a response file so that `tcc` will run it later.
        //
        // We have a tool for this: `moon tool write-tcc-rsp-file <out> <args...>`
        let moonbuild = BINARIES
            .moonbuild
            .to_str()
            .expect("moonbuild path is valid UTF-8");
        let mut rsp_cmdline = vec![
            moonbuild.to_string(),
            "tool".to_string(),
            "write-tcc-rsp-file".to_string(),
        ];
        let rsp_path = products.single_output_path();

        rsp_cmdline.push(rsp_path.display().to_string());
        rsp_cmdline.extend(cmdline.into_iter().skip(1)); // skip original `tcc` command

        BuildCommand {
            extra_inputs: vec![],
            commandline: rsp_cmdline.into(),
        }
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn lower_parse_mbti(&mut self, pid: PackageId) -> BuildCommand {
        let pkg = self.packages.get_package(pid);
        let Some(mbti_path) = &pkg.virtual_mbti else {
            panic!(
                "Lowering ParseMbti node for non-virtual package {}, this is a bug",
                pkg.fqn
            );
        };

        // The virtual package interface is emitted as the `.mi` of the source target
        let target = pid.build_target(TargetKind::Source);
        let mi_out = self.artifact_paths.mi_of_build_target(
            self.packages,
            &target,
            self.opt.target_backend.into(),
        );

        // Resolve interface dependencies from the dep graph (path:alias pairs)
        let mi_inputs = self.mi_inputs_of(target);

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
                moonutil::toolchain::core_bundle_in(stdlib_root, self.opt.target_backend.into())
                    .into(),
            );
        }

        BuildCommand {
            // Track the user-written `.mbti` contract as an explicit input
            extra_inputs: vec![mbti_path.clone()],
            commandline: cmd.build_command(&*BINARIES.moonc).into(),
        }
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn mi_inputs_of(&self, target: BuildTarget) -> Vec<MiDependency<'a>> {
        let mut deps: Vec<MiDependency<'a>> = self
            .rel
            .dep_graph
            .edges_directed(target, Direction::Outgoing)
            .map(|(_, it, w)| {
                let in_file = self.artifact_paths.mi_of_build_target(
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
