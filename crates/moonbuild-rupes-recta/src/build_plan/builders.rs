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

//! Individual build methods for different node types.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use indexmap::{IndexSet, set::MutableValues};
use moonutil::{
    build_options::RunMode,
    compiler_flags::{self, CC, Toolchain, ToolchainSource},
    cond_expr::OptLevel,
    constants::{
        MBTI_USER_WRITTEN, MOD_DIR, MOONCAKE_BIN, PKG_DIR, PackageSourceFileKind, is_moon_mod,
        is_moon_pkg, package_source_file_kind,
    },
    manifest::{MoonMod, MoonModRule},
    package::{MoonPkgGenerate, SupportedTargetsDeclKind},
    resolution::ModuleId,
    scripts::{IgnoredMoonScript, is_moon_script_ignored},
    toolchain,
};
use regex::Regex;
use relative_path::{PathExt, RelativePath};
use tracing::{Level, debug, instrument, trace, warn};

use crate::{
    build_plan::{BuildBundleInfo, PackagePrebuildPolicy, PrebuildInfo},
    cond_comp,
    discover::DiscoveredPackage,
    model::{
        BackendConfig, BuildPlanNode, BuildTarget, DirectNativeMode, NativeBackendMode,
        NativeTarget, OperatingSystem, PackageId, TargetKind,
    },
    pkg_name::PackageFQNWithSource,
};

use super::{
    BuildCStubsInfo, BuildPlanConstructError, BuildRuntimeInfo, BuildTargetInfo, LinkCoreInfo,
    MakeExecutableInfo, PackagePrebuildKey,
    artifact::{ArtifactKey, package_file_key, runtime_source_key},
    c_stub_archive_fingerprint,
    constructor::{BuildPlanConstructor, PackageFileSet},
    runtime_archive_fingerprint,
};

static BUILD_VAR_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$\{build\.([a-zA-Z0-9_]+)\}").expect("invalid build var regex"));
const PROOF_ENABLED_WARN_SUPPRESSIONS: &str = "-1-2-3-29";

fn should_generate_llvm_dsym(debug_symbols: bool, os: OperatingSystem) -> bool {
    debug_symbols && os == OperatingSystem::MacOS
}

fn should_generate_direct_native_dsym(
    mode: &DirectNativeMode,
    debug_symbols: bool,
    toolchain: &Toolchain,
) -> bool {
    debug_symbols
        && mode.target() == NativeTarget::Aarch64AppleDarwin
        && toolchain.cc().targets_apple_darwin()
}

impl<'a> BuildPlanConstructor<'a> {
    fn new_native_linker_context(&self, err: anyhow::Error) -> anyhow::Error {
        if self.build_env.direct_native_target() == Some(NativeTarget::X86_64PcWindowsMsvc) {
            err.context(
                "Windows direct-object native backend requires MSVC (cl.exe or clang-cl.exe); \
                 MinGW/GCC is supported via the generated-C backend (unset MOONBIT_NEW_NATIVE)",
            )
        } else if self.build_env.direct_native_target().is_some() {
            err.context(
                "new native backend requires a C compiler/linker driver; install clang/cc or set MOON_CC",
            )
        } else {
            err
        }
    }

    fn warn_incompatible_windows_msvc_env_override(&mut self) {
        if self.warned_incompatible_windows_msvc_env_override
            || !compiler_flags::has_incompatible_windows_msvc_env_override()
        {
            return;
        }

        self.warned_incompatible_windows_msvc_env_override = true;
        self.user_log.warn(
            "MOON_CC is ignored for Windows direct-object native target because it is not a \
             cl-compatible driver; set MOON_CC to cl.exe or clang-cl.exe to override MSVC \
             discovery, or unset MOONBIT_NEW_NATIVE to use the generated-C backend which \
             supports MinGW/GCC.",
        );
    }

    fn effective_native_toolchain(&mut self, package_cc: Option<&CC>) -> anyhow::Result<Toolchain> {
        debug_assert!(self.build_env.target_backend().is_native());
        if self.build_env.direct_native_target() == Some(NativeTarget::X86_64PcWindowsMsvc) {
            self.warn_incompatible_windows_msvc_env_override();
            return compiler_flags::windows_msvc_native_toolchain(package_cc);
        }

        compiler_flags::effective_native_toolchain(
            package_cc,
            self.build_env.tcc_run().map(|config| config.internal_tcc()),
        )
    }

    pub(super) fn warn_moon_cc_overrides(&self) {
        if !self.build_env.target_backend().is_native() {
            return;
        }

        for (package, pkg) in self.input.pkg_dirs.all_packages(true) {
            let Some(native) = pkg.raw.link.as_ref().and_then(|link| link.native.as_ref()) else {
                continue;
            };
            let cc_overridden = native.cc.is_some()
                && self
                    .res
                    .backend
                    .make_executable_info
                    .iter()
                    .any(|(target, info)| {
                        target.package == package
                            && info.effective_native_toolchain.source()
                                == ToolchainSource::EnvOverride
                    });
            let stub_cc_overridden = native.stub_cc.is_some()
                && self
                    .res
                    .backend
                    .c_stubs_info
                    .get(&package)
                    .is_some_and(|info| {
                        info.effective_native_toolchain.source() == ToolchainSource::EnvOverride
                    });
            let fields = [
                cc_overridden.then_some("`link.native.cc`"),
                stub_cc_overridden.then_some("`link.native.stub-cc`"),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
            if !fields.is_empty() {
                self.user_log.warn(format!(
                    "`MOON_CC` overrides {} configured by package `{}`.",
                    fields.join(" and "),
                    pkg.fqn
                ));
            }
        }
    }

    fn module_prebuild_vars(&self, module: ModuleId) -> Option<&HashMap<String, String>> {
        self.prebuild_config
            .and_then(|cfg| cfg.module_outputs.get(&module))
            .map(|output| &output.vars)
    }

    fn replace_build_vars<'s>(
        &self,
        package: PackageId,
        module: ModuleId,
        value: &'s str,
    ) -> Cow<'s, str> {
        let Some(vars) = self.module_prebuild_vars(module) else {
            return Cow::Borrowed(value);
        };
        if vars.is_empty() {
            return Cow::Borrowed(value);
        }
        BUILD_VAR_REGEX.replace_all(value, |caps: &regex::Captures| {
            vars.get(caps.get(1).expect("build var regex has capture").as_str())
                .map(|s| s.as_str())
                .unwrap_or_else(|| {
                    let m_name = self.input.module_rel.module_source(module);
                    let pkg_name = &self.input.pkg_dirs.get_package(package).fqn;
                    warn!(
                        "Build variable {} required in {} but not found in \
                        prebuild config output of module {}, \
                         replacing with empty string",
                        &caps[1], pkg_name, m_name
                    );

                    ""
                })
        })
    }

    /// Plan all package-level file generation needed by this backend plan.
    ///
    /// According to the semantics, only local packages require package-level
    /// prebuild to run. Remote packages should already contain their outputs.
    fn plan_package_prebuild(&mut self, pkg_id: PackageId) -> Result<(), BuildPlanConstructError> {
        let pkg = self.input.pkg_dirs.get_package(pkg_id);
        if self.input_directive.package_prebuild != PackagePrebuildPolicy::Run
            || !self.input.local_modules().contains(&pkg.module)
        {
            return Ok(());
        }

        let discovered_moonlex_inputs = pkg.mbt_lex_files.clone();
        let discovered_moonyacc_inputs = pkg.mbt_yacc_files.clone();
        let custom_rules = pkg.raw.pre_build.as_deref().unwrap_or_default();

        if !is_moon_script_ignored(IgnoredMoonScript::Prebuild) {
            let custom_actions = custom_rules
                .iter()
                .enumerate()
                .filter(|(index, _)| {
                    let key = PackagePrebuildKey::Custom {
                        package: pkg_id,
                        declaration_index: *index as u32,
                    };
                    !self.res.package_prebuild.contains_key(&key)
                })
                .map(|(index, command)| {
                    self.resolve_custom_prebuild(pkg_id, command)
                        .map(|info| (index as u32, info))
                })
                .collect::<Result<Vec<_>, _>>()?;
            for (index, info) in custom_actions {
                self.res.package_prebuild.insert_custom(pkg_id, index, info);
            }
        }

        // A custom prebuild may itself generate moonlex/moonyacc input. Treat
        // those paths exactly like files observed during package discovery;
        // matching input and output paths connect their execution actions.
        let custom_outputs = self
            .res
            .package_prebuild
            .custom_output_paths(pkg_id)
            .cloned()
            .collect::<Vec<_>>();
        let mut moonlex_inputs = discovered_moonlex_inputs
            .into_iter()
            .collect::<IndexSet<_>>();
        let mut moonyacc_inputs = discovered_moonyacc_inputs
            .into_iter()
            .collect::<IndexSet<_>>();
        for output in custom_outputs {
            let Some(kind) = output
                .file_name()
                .and_then(OsStr::to_str)
                .and_then(package_source_file_kind)
            else {
                continue;
            };
            match kind {
                PackageSourceFileKind::Mbl => {
                    moonlex_inputs.insert(output);
                }
                PackageSourceFileKind::Mby => {
                    moonyacc_inputs.insert(output);
                }
                PackageSourceFileKind::Mbt
                | PackageSourceFileKind::MbtMd
                | PackageSourceFileKind::Mbtp => {}
            }
        }

        for input in moonlex_inputs {
            let output = input.with_extension("mbt");
            self.res
                .package_prebuild
                .insert_moonlex(pkg_id, input, output);
        }
        for input in moonyacc_inputs {
            let output = input.with_extension("mbt");
            self.res
                .package_prebuild
                .insert_moonyacc(pkg_id, input, output);
        }
        Ok(())
    }

    fn check_backend_compatibility_for_dep(
        &mut self,
        importer_target: BuildTarget,
        dep: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        let selected_backend = self.build_env.target_backend();
        let importer_pkg = self.input.pkg_dirs.get_package(importer_target.package);
        let dependency_pkg = self.input.pkg_dirs.get_package(dep.package);

        if importer_pkg.single_file_source_kind.is_none()
            && importer_pkg.supported_targets_decl == SupportedTargetsDeclKind::Omitted
            && importer_target.package != dep.package
            && dependency_pkg.supported_targets_decl != SupportedTargetsDeclKind::Omitted
            && self
                .warned_missing_supported_targets
                .insert(importer_target.package)
        {
            self.user_log.warn(format!(
                "Package `{}` does not declare `supported_targets`, but depends on `{}` which declares it. Consider declaring `supported_targets` explicitly",
                importer_pkg.fqn, dependency_pkg.fqn
            ));
        }

        let dependency_realizable = self
            .input
            .pkg_rel
            .realizable_supported_targets
            .get(&dep)
            .expect("realizable backend support should be available for every dependency node");

        if dependency_realizable.contains(&selected_backend) {
            return Ok(());
        }

        let mut supported_backends = dependency_realizable
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        supported_backends.sort();

        Err(BuildPlanConstructError::BackendIncompatibleDependency {
            backend: selected_backend,
            importer: importer_pkg.fqn.to_string(),
            dependency: dependency_pkg.fqn.to_string(),
            supported_backends: format!("[{}]", supported_backends.join(", ")),
            path: format!("{} -> {}", importer_pkg.fqn, dependency_pkg.fqn),
        })
    }

    /// Validate backend compatibility for a dependency edge that needs `.mi`.
    ///
    /// Callers choose policy (hard error vs warning+skip) before mutating the graph.
    ///
    /// Note: This mirrors the stdlib short-circuit used by the artifact helpers.
    /// When stdlib is injected, stdlib package deps are not planned and should
    /// not be backend-checked here either.
    fn check_backend_compatibility_for_mi_dep(
        &mut self,
        node: BuildPlanNode,
        dep: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        if self.build_env.std && self.input.pkg_dirs.is_stdlib_package(dep.package) {
            return Ok(());
        }

        let importer_target = match node {
            BuildPlanNode::BuildVirtual(pkg) => Some(pkg.build_target(TargetKind::Source)),
            _ => node.extract_target(),
        };

        if let Some(importer_target) = importer_target {
            self.check_backend_compatibility_for_dep(importer_target, dep)?;
        }

        Ok(())
    }

    fn require_check_mi_of_dep(&mut self, node: BuildPlanNode, dep: BuildTarget) {
        if self.build_env.std && self.input.pkg_dirs.is_stdlib_package(dep.package) {
            return;
        }

        let pkg_info = self.input.pkg_dirs.get_package(dep.package);
        let artifact = if pkg_info.is_virtual() {
            ArtifactKey::VirtualContractMi {
                package: dep.package,
            }
        } else {
            ArtifactKey::CheckMi {
                package: dep.package,
                target_kind: dep.kind,
            }
        };
        self.require_artifact(node, artifact);
    }

    fn require_build_mi_of_dep(&mut self, node: BuildPlanNode, dep: BuildTarget) {
        if self.build_env.std && self.input.pkg_dirs.is_stdlib_package(dep.package) {
            return;
        }

        let pkg_info = self.input.pkg_dirs.get_package(dep.package);
        let artifact = if pkg_info.is_virtual() {
            ArtifactKey::VirtualContractMi {
                package: dep.package,
            }
        } else {
            ArtifactKey::BuildMi {
                package: dep.package,
                target_kind: dep.kind,
            }
        };
        self.require_artifact(node, artifact);
    }

    fn require_build_outputs_of_dep(&mut self, node: BuildPlanNode, dep: BuildTarget) {
        if self.build_env.std && self.input.pkg_dirs.is_stdlib_package(dep.package) {
            return;
        }

        let pkg_info = self.input.pkg_dirs.get_package(dep.package);
        if pkg_info.is_virtual() {
            self.require_artifact(
                node,
                ArtifactKey::VirtualContractMi {
                    package: dep.package,
                },
            );
            return;
        }

        self.require_artifact(
            node,
            ArtifactKey::BuildMi {
                package: dep.package,
                target_kind: dep.kind,
            },
        );
        self.require_artifact(
            node,
            ArtifactKey::CoreIr {
                package: dep.package,
                target_kind: dep.kind,
            },
        );
    }

    /// Specify a need on the proof artifacts of a dependency.
    ///
    /// Dependency proofs stay modular: dependents only require the dependency's
    /// proof surface (`.mi` + `.mlw`). Provider selection decides whether an
    /// explicit `Prove` or an internal `EmitProof` action supplies that surface.
    fn need_proof_of_dep(&mut self, node: BuildPlanNode, dep: BuildTarget) {
        // As with normal `.mi` dependencies, stdlib packages are resolved via
        // the injected stdlib path rather than by planning local nodes.
        if self.build_env.std && self.input.pkg_dirs.is_stdlib_package(dep.package) {
            return;
        }

        self.require_artifact(
            node,
            ArtifactKey::ProofMi {
                package: dep.package,
                target_kind: dep.kind,
            },
        );
        self.require_artifact(
            node,
            ArtifactKey::ProofWhyml {
                package: dep.package,
                target_kind: dep.kind,
            },
        );
    }

    fn need_virtual_if_necessary(
        &mut self,
        pkg: &DiscoveredPackage,
        node: BuildPlanNode,
        target: BuildTarget,
    ) {
        // If the given target is a virtual package with default implementation,
        // we need to build its interface first. Injected stdlib contracts are
        // already supplied by `-std-path` and remain external to this plan.
        if pkg.is_virtual() && !(self.build_env.std && pkg.is_stdlib) {
            self.require_artifact(
                node,
                ArtifactKey::VirtualContractMi {
                    package: target.package,
                },
            );
        }

        // If the given target implements a virtual package, we need to build
        // the virtual package's interface first, unless that contract comes
        // from the injected stdlib.
        if let Some(vpkg_id) = self.input.pkg_rel.virt_impl.get(target.package)
            && !(self.build_env.std && self.input.pkg_dirs.is_stdlib_package(*vpkg_id))
        {
            self.require_artifact(node, ArtifactKey::VirtualContractMi { package: *vpkg_id });
        }
    }

    pub(crate) fn build_proof_node(
        &mut self,
        node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        let pkg = self.input.pkg_dirs.get_package(target.package);

        assert!(
            pkg.has_implementation(),
            "Building proof for a virtual package without implementation should use the \
            `BuildVirtual` action instead"
        );

        self.need_node(node);
        for dep in self
            .input
            .pkg_rel
            .dep_graph
            .neighbors_directed(target, petgraph::Direction::Outgoing)
        {
            self.check_backend_compatibility_for_mi_dep(node, dep)?;
            self.need_proof_of_dep(node, dep);
        }

        self.plan_package_prebuild(target.package)?;
        self.need_virtual_if_necessary(pkg, node, target);
        self.populate_target_info(target);
        self.resolved_node(node);

        Ok(())
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_check(
        &mut self,
        node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        let pkg = self.input.pkg_dirs.get_package(target.package);

        assert!(
            pkg.has_implementation(),
            "Checking a virtual package without implementation should use the \
            `BuildVirtual` action instead"
        );

        self.need_node(node);
        // Check depends on `.mi` of all dependencies, which practically
        // means the Check of all dependencies.
        for dep in self
            .input
            .pkg_rel
            .dep_graph
            .neighbors_directed(target, petgraph::Direction::Outgoing)
        {
            self.check_backend_compatibility_for_mi_dep(node, dep)?;
            self.require_check_mi_of_dep(node, dep);
        }

        self.plan_package_prebuild(target.package)?;

        self.need_virtual_if_necessary(pkg, node, target);
        self.populate_target_info(target);

        self.resolved_node(node);

        Ok(())
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_build(
        &mut self,
        node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        let pkg = self.input.pkg_dirs.get_package(target.package);

        assert!(
            pkg.has_implementation(),
            "Building a virtual package without implementation should use the \
            `BuildVirtual` action instead"
        );

        // Build consumes `.mi` compiler inputs from all dependencies. It also
        // tracks normal dependency `.core` artifacts in n2 so implementation
        // changes in dependencies dirty downstream build-package actions.
        self.need_node(node);
        for dep in self
            .input
            .pkg_rel
            .dep_graph
            .neighbors_directed(target, petgraph::Direction::Outgoing)
        {
            self.check_backend_compatibility_for_mi_dep(node, dep)?;
            self.require_build_outputs_of_dep(node, dep);
        }

        // If the given target is a test, we will also need to generate the test driver.
        if target.kind.is_test() {
            self.require_artifact(
                node,
                ArtifactKey::GeneratedTestDriver {
                    package: target.package,
                    target_kind: target.kind,
                },
            );
        }

        self.need_virtual_if_necessary(pkg, node, target);

        self.plan_package_prebuild(target.package)?;

        self.populate_target_info(target);
        self.resolved_node(node);

        Ok(())
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_gen_test_info(
        &mut self,
        node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        self.need_node(node);

        self.plan_package_prebuild(target.package)?;
        self.populate_target_info(target);
        self.resolved_node(node);
        Ok(())
    }

    fn package_file_set(&mut self, package: PackageId) -> &PackageFileSet {
        if !self.package_file_sets.contains_key(&package) {
            let file_set = self.collect_package_file_set(package);
            self.package_file_sets.insert(package, file_set);
        }
        self.package_file_sets
            .get(&package)
            .expect("package file set should be cached after collection")
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    fn collect_package_file_set(&self, package: PackageId) -> PackageFileSet {
        use crate::cond_comp::FileTestKind::*;

        let pkg = self.input.pkg_dirs.get_package(package);

        // `.mbtx` is interpreted as a standalone source during resolution. It
        // is not a package source filename, so it does not participate in
        // package conditional-compilation or test-file classification.
        let is_mbtx_single_file = pkg.is_mbtx_single_file();
        let source_files_for_classification = if is_mbtx_single_file {
            &[][..]
        } else {
            pkg.source_files.as_slice()
        };
        let mut planned_prebuild_outputs = self
            .res
            .package_prebuild
            .output_paths(package)
            .cloned()
            .collect::<IndexSet<_>>();

        let mut generated_sources = Vec::new();
        let mut mbt_md_files = pkg.mbt_md_files.iter().cloned().collect::<IndexSet<_>>();
        let mut mbtp_files = pkg.mbtp_files.iter().cloned().collect::<IndexSet<_>>();
        for output in planned_prebuild_outputs.drain(..) {
            let Some(kind) = output
                .file_name()
                .and_then(OsStr::to_str)
                .and_then(package_source_file_kind)
            else {
                continue;
            };
            match kind {
                PackageSourceFileKind::Mbt => {
                    generated_sources.push(output);
                }
                PackageSourceFileKind::MbtMd => {
                    mbt_md_files.insert(output);
                }
                PackageSourceFileKind::Mbtp => {
                    mbtp_files.insert(output);
                }
                // Inputs for built-in generators have already been interpreted
                // while planning PackagePrebuildPlan. Their `.mbt` outputs are
                // present in the same output set and flow through classification.
                PackageSourceFileKind::Mbl | PackageSourceFileKind::Mby => {}
            }
        }

        let source_iter = source_files_for_classification
            .iter()
            .map(|x| Cow::Borrowed(x.as_path()))
            .chain(generated_sources.into_iter().map(Cow::Owned));

        let mut no_test_files = IndexSet::new();
        let mut whitebox_files = IndexSet::new();
        let mut blackbox_files = IndexSet::new();

        if is_mbtx_single_file {
            no_test_files.extend(pkg.source_files.iter().cloned());
        }

        let _classify_span = tracing::debug_span!("classifying_package_files").entered();
        for (file, file_kind) in cond_comp::classify_files(
            &pkg.raw,
            source_iter,
            self.build_env.opt_level,
            self.build_env.target_backend(),
        ) {
            match file_kind {
                NoTest => no_test_files.insert(file.into_owned()),
                Whitebox => whitebox_files.insert(file.into_owned()),
                Blackbox => blackbox_files.insert(file.into_owned()),
            };
        }
        drop(_classify_span);

        // Discovery sorts authored paths, but generated outputs are appended in
        // declaration order during File Interpretation. Sort the final Build
        // Target Projection so compiler arguments and n2 inputs are stable.
        let _sort_span = tracing::debug_span!("sorting_package_files").entered();
        // `.mbt.md` is always a blackbox regular input. At this point authored
        // and generated paths have the same Build Target Projection semantics.
        blackbox_files.extend(mbt_md_files);
        no_test_files.sort();
        whitebox_files.sort();
        blackbox_files.sort();
        mbtp_files.sort();
        drop(_sort_span);

        PackageFileSet {
            no_test_files: no_test_files.into_iter().collect(),
            whitebox_files: whitebox_files.into_iter().collect(),
            blackbox_files: blackbox_files.into_iter().collect(),
            mbtp_files: mbtp_files.into_iter().collect(),
        }
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn resolve_mbt_files_for_node(&mut self, target: BuildTarget) -> BuildTargetInfo {
        use TargetKind::*;

        // FIXME: Should we resolve test drivers' paths, or should we leave it
        // in the lowering phase? The path to the test driver depends on the
        // artifact layout, so we might not be able to do that here, unless we
        // add some kind of `SpecialFile::TestDriver` or something.
        let (regular_files, mbtp_files, whitebox_files, doctest_files) = {
            // Discovery keeps `.mbtp` files for metadata, but only verification
            // commands project them into compiler inputs.
            let uses_mbtp = matches!(
                self.build_env.action,
                moonutil::build_options::RunMode::Check | moonutil::build_options::RunMode::Prove
            );
            let file_set = self.package_file_set(target.package);
            let mbtp_files = if uses_mbtp {
                file_set.mbtp_files.clone()
            } else {
                Vec::new()
            };
            match target.kind {
                Source | SubPackage | InlineTest => (
                    file_set.no_test_files.clone(),
                    mbtp_files,
                    Vec::new(),
                    Vec::new(),
                ),
                WhiteboxTest => (
                    file_set.no_test_files.clone(),
                    mbtp_files,
                    file_set.whitebox_files.clone(),
                    Vec::new(),
                ),
                BlackboxTest => (
                    file_set.blackbox_files.clone(),
                    Vec::new(),
                    Vec::new(),
                    file_set.no_test_files.clone(),
                ),
            }
        };

        let pkg = self.input.pkg_dirs.get_package(target.package);
        let module = self.input.module_info(pkg.module);

        // Populate `warn_list` by concatenating module-level, package-level,
        // and command-line settings.
        let proof_warn_list = (pkg.raw.proof_enabled
            && !matches!(
                self.build_env.action,
                moonutil::build_options::RunMode::Check | moonutil::build_options::RunMode::Prove
            ))
        .then_some(PROOF_ENABLED_WARN_SUPPRESSIONS);
        let package_warn_list = cat_opt(pkg.raw.warn_list.clone(), proof_warn_list);
        let warn_list = cat_opt(
            cat_opt(module.warn_list.clone(), package_warn_list.as_deref()),
            self.build_env.warn_list.as_deref(),
        );

        let specified_no_mi = self.input_directive.specify_no_mi_for == Some(target.package);
        let patch_file = self
            .input_directive
            .specify_patch_file
            .as_ref()
            .filter(|(specify_target, _)| specify_target == &target)
            .map(|(_, path)| path.clone());
        let why3_config = self.input_directive.prove_why3_config.clone();
        let proof_prelude = self.input_directive.proof_prelude.clone();

        let mi_check_target = self.mi_check_target(target, pkg);

        BuildTargetInfo {
            regular_files,
            mbtp_files,
            whitebox_files,
            doctest_files,
            warn_list,
            specified_no_mi,
            patch_file,
            why3_config,
            proof_prelude,
            check_mi_against: mi_check_target,
            value_tracing: self
                .input_directive
                .value_tracing
                .is_some_and(|pkg| pkg == target.package),
        }
    }

    pub(super) fn warn_if_main_package_uses_blackbox_inputs(
        &mut self,
        pkg: &DiscoveredPackage,
        regular_files: &[PathBuf],
    ) {
        if !pkg.raw.is_main {
            return;
        }

        let mut blackbox_inputs = Vec::new();

        if regular_files.iter().any(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    package_source_file_kind(name) == Some(PackageSourceFileKind::Mbt)
                        && matches!(
                            cond_comp::get_file_test_kind_full(name),
                            cond_comp::FileTestKind::Blackbox
                        )
                })
        }) {
            blackbox_inputs.push("`_test.mbt` files");
        }

        if regular_files.iter().any(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    package_source_file_kind(name) == Some(PackageSourceFileKind::MbtMd)
                })
        }) {
            blackbox_inputs.push("`.mbt.md` files");
        }

        if blackbox_inputs.is_empty() {
            return;
        }

        self.user_log.warn(format!(
            "Main package `{}` uses blackbox-only test inputs ({}) in package directory \"{}\". \
             Main packages will stop generating blackbox tests in a future release. \
             Move public behavior into a non-main package and keep the main package as an entrypoint.",
            pkg.fqn,
            blackbox_inputs.join(", "),
            pkg.root_path.display(),
        ));
    }

    /// Check if a given target needs to check `.mi` against another target.
    #[allow(clippy::manual_map)]
    fn mi_check_target(&self, target: BuildTarget, pkg: &DiscoveredPackage) -> Option<BuildTarget> {
        // Mi checks.
        // - A virtual package with a default implementation checks .mi with its
        //   own virtual interface declaration.
        // - A package implementing a virtual package checks .mi with the
        //   virtual package it implements.
        if target.kind == TargetKind::Source {
            if let Some(vpkg) = &pkg.raw.virtual_pkg {
                if vpkg.has_default {
                    Some(target.package.build_target(TargetKind::Source))
                } else {
                    unreachable!(
                        "A virtual package without default implementation should not have a build target info, thus should not reach here"
                    );
                }
            } else if let Some(implement) = self.input.pkg_rel.virt_impl.get(target.package) {
                Some(implement.build_target(TargetKind::Source))
            } else {
                None
            }
        } else {
            None
        }
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_build_c_stub(
        &mut self,
        node: BuildPlanNode,
        _target: PackageId,
        _index: u32,
    ) -> Result<(), BuildPlanConstructError> {
        // depends on nothing, but needs to be inserted into the list
        self.need_node(node);

        // We rely on the `link_c_stubs` action to resolve the C stub info
        // so this doesn't panic.
        self.resolved_node(node);
        Ok(())
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_archive_or_link_c_stubs(
        &mut self,
        node: BuildPlanNode,
        target: PackageId,
    ) -> Result<(), BuildPlanConstructError> {
        // Resolve the C stub files
        let pkg = self.input.pkg_dirs.get_package(target);
        for source in &pkg.c_stub_files {
            self.require_artifact(
                node,
                ArtifactKey::CStubObject {
                    package: target,
                    source: package_file_key(&pkg.root_path, source),
                },
            );
        }

        // If we're tcc run, also depend on the runtime library
        if self.build_env.tcc_run().is_some() {
            self.require_artifact(node, ArtifactKey::RuntimeLibrary);
        }

        // Populate C stub info
        let native_config = pkg.raw.link.as_ref().and_then(|x| x.native.as_ref());

        let stub_cc = native_config
            .and_then(|native| native.stub_cc.as_ref())
            .map(|s| self.replace_build_vars(target, pkg.module, s))
            .map(|replaced| {
                CC::try_from_path(replaced.as_ref()).map_err(|e| {
                    BuildPlanConstructError::FailedToSetStubCC(e, pkg.fqn.clone().into())
                })
            })
            .transpose()?;

        let cc_flags = native_config
            .and_then(|native| native.stub_cc_flags.as_ref())
            .map(|s| self.replace_build_vars(target, pkg.module, s))
            .map(|replaced| {
                shlex::split(replaced.as_ref()).ok_or_else(|| {
                    BuildPlanConstructError::MalformedStubCCFlags(pkg.fqn.clone().into())
                })
            })
            .transpose()?
            .unwrap_or_default();

        let mut link_flags = native_config
            .and_then(|native| native.stub_cc_link_flags.as_ref())
            .map(|s| self.replace_build_vars(target, pkg.module, s))
            .map(|replaced| {
                shlex::split(replaced.as_ref()).ok_or_else(|| {
                    BuildPlanConstructError::MalformedStubCCLinkFlags(pkg.fqn.clone().into())
                })
            })
            .transpose()?
            .unwrap_or_default();

        let effective_native_toolchain = self
            .effective_native_toolchain(stub_cc.as_ref())
            .map_err(|e| {
                BuildPlanConstructError::FailedToSetStubCC(
                    self.new_native_linker_context(e),
                    pkg.fqn.clone().into(),
                )
            })?;

        self.propagate_link_config(
            &effective_native_toolchain,
            std::iter::once(target),
            &mut link_flags,
        );

        let static_archive_fingerprint = (self.build_env.tcc_run().is_none()
            && effective_native_toolchain
                .cc()
                .archiver_updates_existing_archive())
        .then(|| c_stub_archive_fingerprint(&pkg.c_stub_files));

        let c_info = BuildCStubsInfo {
            effective_native_toolchain,
            cc_flags,
            link_flags,
            static_archive_fingerprint,
        };
        self.res.backend.c_stubs_info.insert(target, c_info);
        self.resolved_node(node);

        Ok(())
    }

    /// Plan a `LinkCore` action and its transitive Core IR inputs.
    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_link_core(
        &mut self,
        node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        /*
            Link-core requires traversing all output of the current package's
            all transitive dependencies, and emitting them in DFS post-order.

            There are a couple of replacements needed to be done when the
            traversal completes:
            - Whitebox tests need to replace the normal package in the
                dependency graph (at the same position as the normal package).
                This is technically a circular dependency but anyway :)
            - Virtual package overrides need to replace their overridden
                packages in the dependency graph. This is done by not adding
                virtual packages at all when collecting the targets.
        */

        debug!("Linking Core IR for target: {:?}", target);
        debug!("Performing DFS post-order traversal to collect dependencies");

        // This DFS is shared by both LinkCore and MakeExecutable actions.
        let (link_core_deps, _, abort_overridden) = self.dfs_link_core_sources(target)?;

        // The traversal has already replaced unnecessary dependencies.
        for target in &link_core_deps {
            self.require_artifact(
                node,
                ArtifactKey::CoreIr {
                    package: target.package,
                    target_kind: target.kind,
                },
            );
        }

        // Use DFS-built order directly (dependencies first, then dependents).
        let targets = link_core_deps.iter().copied().collect::<Vec<_>>();
        let link_core_info = LinkCoreInfo {
            linked_order: targets,
            abort_overridden,
            // std: self.build_env.std, // Can move std/nostd to per-package info
        };
        self.res
            .backend
            .link_core_info
            .insert(target, link_core_info);
        self.resolved_node(node);

        Ok(())
    }

    /// Plan the native toolchain action that turns linked Core output into an executable.
    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_native_executable(
        &mut self,
        node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        debug_assert!(self.build_env.target_backend().is_native());

        let (link_core_deps, c_stub_deps, _) = self.dfs_link_core_sources(target)?;
        let targets = link_core_deps.into_iter().collect::<Vec<_>>();

        self.require_artifact(
            node,
            ArtifactKey::LinkedCore {
                package: target.package,
                target_kind: target.kind,
            },
        );

        // Add dependencies of make exec
        for target in &c_stub_deps {
            self.require_artifact(node, ArtifactKey::CStubLibrary { package: *target });
        }
        let c_stub_deps = c_stub_deps.into_iter().collect::<Vec<_>>();

        // Fill auxiliary flags for CC flags
        let pkg = self.input.pkg_dirs.get_package(target.package);
        let native_config = pkg.raw.link.as_ref().and_then(|x| x.native.as_ref());
        let cc = native_config
            .and_then(|native| native.cc.as_ref())
            .map(|s| self.replace_build_vars(target.package, pkg.module, s))
            .map(|replaced| {
                CC::try_from_path(replaced.as_ref())
                    .map_err(|e| BuildPlanConstructError::FailedToSetCC(e, pkg.fqn.clone().into()))
            })
            .transpose()?;
        let c_flags = native_config
            .and_then(|native| native.cc_flags.as_ref())
            .map(|s| self.replace_build_vars(target.package, pkg.module, s))
            .map(|replaced| {
                shlex::split(replaced.as_ref()).ok_or_else(|| {
                    BuildPlanConstructError::MalformedCCFlags(pkg.fqn.clone().into())
                })
            })
            .transpose()?
            .unwrap_or_default();

        let mut link_flags = native_config
            .and_then(|native| native.cc_link_flags.as_ref())
            .map(|s| self.replace_build_vars(target.package, pkg.module, s))
            .map(|replaced| {
                shlex::split(replaced.as_ref()).ok_or_else(|| {
                    BuildPlanConstructError::MalformedCCLinkFlags(pkg.fqn.clone().into())
                })
            })
            .transpose()?
            .unwrap_or_default();

        let mut link_pkgs: Vec<PackageId> = targets.iter().map(|x| x.package).collect();
        if !link_pkgs.contains(&target.package) {
            link_pkgs.push(target.package);
        }

        let effective_native_toolchain =
            self.effective_native_toolchain(cc.as_ref()).map_err(|e| {
                BuildPlanConstructError::FailedToSetCC(
                    self.new_native_linker_context(e),
                    pkg.fqn.clone().into(),
                )
            })?;

        self.propagate_link_config(
            &effective_native_toolchain,
            link_pkgs.into_iter(),
            &mut link_flags,
        );

        let generate_dsym = match &self.build_env.backend {
            BackendConfig::Llvm => {
                should_generate_llvm_dsym(self.build_env.debug_symbols, self.build_env.os)
            }
            BackendConfig::Native(NativeBackendMode::DirectObject(mode)) => {
                should_generate_direct_native_dsym(
                    mode,
                    self.build_env.debug_symbols,
                    &effective_native_toolchain,
                )
            }
            BackendConfig::Native(NativeBackendMode::GeneratedC | NativeBackendMode::TccRun(_)) => {
                false
            }
            BackendConfig::Wasm { .. } | BackendConfig::WasmGc { .. } | BackendConfig::Js => {
                unreachable!("non-native executable planning returns before toolchain planning")
            }
        };
        if generate_dsym && self.res.backend.dsymutil.is_none() {
            self.res.backend.dsymutil = Some(
                moonutil::toolchain::resolve_executable("dsymutil").map_err(|error| {
                    BuildPlanConstructError::FailedToResolveDsymutil(error, pkg.fqn.clone().into())
                })?,
            );
        }

        let v = MakeExecutableInfo {
            link_c_stubs: c_stub_deps.clone(),
            effective_native_toolchain,
            c_flags,
            link_flags,
        };
        self.res.backend.make_executable_info.insert(target, v);

        self.require_artifact(node, ArtifactKey::RuntimeLibrary);

        if generate_dsym {
            let dsym_node = self.need_node(BuildPlanNode::GenerateDsym(target));
            self.require_artifact(
                dsym_node,
                ArtifactKey::Executable {
                    package: target.package,
                    target_kind: target.kind,
                },
            );
            self.resolved_node(dsym_node);
        }

        self.resolved_node(node);

        Ok(())
    }

    fn dfs_link_core_sources(
        &self,
        target: BuildTarget,
    ) -> Result<(IndexSet<BuildTarget>, IndexSet<PackageId>, bool), BuildPlanConstructError> {
        // This DFS is shared by both LinkCore and MakeExecutable actions.
        let vp_info = self.input.pkg_rel.virtual_users.get(target.package);

        let abort = if self.build_env.std {
            self.input.pkg_dirs.abort_pkg()
        } else {
            None
        };
        let abort_override_pkg =
            abort.and_then(|abort| vp_info.and_then(|vu| vu.overrides.get(abort).copied()));

        // This is the link core sources
        let mut link_core_deps: IndexSet<BuildTarget> = IndexSet::new();
        // This is the C stub sources
        //
        // Since a package don't have separate C stub for different test targets,
        // we only need to record the package IDs here.
        //
        // Additionally, if we don't dedup it here, we will see C stub for the
        // package itself and its blackbox test target both being added, which
        // is redundant.
        let mut c_stub_deps: IndexSet<PackageId> = IndexSet::new();
        // Whether `moonbitlang/core/abort` is overridden
        let abort_overridden = abort_override_pkg.is_some();

        let graph = &self.input.pkg_rel.dep_graph;

        // Topo sort via DFS postorder
        let mut visited: HashSet<BuildTarget> = HashSet::new(); // pre-order visited
        let mut emitted: HashSet<BuildTarget> = HashSet::new(); // post-order emitted
        let mut stack: Vec<(BuildTarget, bool)> = Vec::new(); // bool = expanded marker

        // Seed with the root target
        stack.push((target, false));

        while let Some((curr, expanded)) = stack.pop() {
            if !expanded {
                // Pre-order processing
                // Check if already visited (before override resolution)
                if visited.contains(&curr) {
                    continue;
                }
                visited.insert(curr);

                // Resolve virtual overrides at pre-order for this node
                let mut node = curr;
                if let Some(vp_info) = vp_info
                    && let Some(&override_pkg) = vp_info.overrides.get(node.package)
                {
                    trace!(
                        from = ?node.package,
                        to = ?override_pkg,
                        "Overriding virtual package"
                    );
                    node = BuildTarget {
                        package: override_pkg,
                        kind: TargetKind::Source,
                    };
                }

                // Skip abort entirely
                if abort.is_some_and(|x| node.package == x) {
                    continue;
                }

                trace!(?node, "Pre-order: push marker, then schedule children");
                // Push post-order marker
                stack.push((node, true));

                // Gather dependencies (outgoing neighbors) and sort deterministically
                let mut deps: Vec<BuildTarget> = graph
                    .neighbors_directed(node, petgraph::Direction::Outgoing)
                    .filter(|dep| {
                        // Skip stdlib packages because they are always linked implicitly
                        // only when stdlib is injected. When building stdlib itself, keep them.
                        !self.build_env.std || !self.input.pkg_dirs.is_stdlib_package(dep.package)
                    })
                    .collect();

                if curr == target
                    && let Some(override_pkg) = abort_override_pkg
                {
                    let override_target = BuildTarget {
                        package: override_pkg,
                        kind: TargetKind::Source,
                    };
                    if !deps.contains(&override_target) {
                        deps.push(override_target);
                    }
                }

                deps.sort_by(|a, b| {
                    let pa = self.input.pkg_dirs.get_package(a.package);
                    let pb = self.input.pkg_dirs.get_package(b.package);
                    pa.fqn.cmp(&pb.fqn).then_with(|| a.kind.cmp(&b.kind))
                });

                // Push dependencies in reverse order so lexicographically smallest is processed first.
                // Skip already-visited nodes as an optimization (they would be filtered by the guard above anyway).
                for dep in deps.into_iter().rev() {
                    if !visited.contains(&dep) {
                        stack.push((dep, false));
                    }
                }
            } else {
                // Post-order: emit after all dependencies
                let cur = curr;

                // Whitebox replacement: if a whitebox test exists, replace the source entry in-place.
                if cur.kind == TargetKind::WhiteboxTest {
                    let source_target = cur.package.build_target(TargetKind::Source);
                    if let Some(source_idx) = link_core_deps.get_index_of(&source_target) {
                        let source_mut = link_core_deps
                            .get_index_mut2(source_idx)
                            .expect("Source index is valid");
                        *source_mut = cur;

                        // Record emitted and collect c-stub if necessary
                        emitted.insert(cur);
                        let pkg = self.input.pkg_dirs.get_package(cur.package);
                        if self.build_env.target_backend().is_native()
                            && !pkg.c_stub_files.is_empty()
                        {
                            c_stub_deps.insert(cur.package);
                        }
                        continue;
                    }
                    // If source not found yet, fall through to regular insertion.
                }

                if emitted.contains(&cur) {
                    continue;
                }

                let pkg = self.input.pkg_dirs.get_package(cur.package);
                if !pkg.has_implementation() {
                    return Err(BuildPlanConstructError::NoImplementationForVirtualPackage {
                        package: self.input.pkg_dirs.fqn(target.package).clone().into(),
                        dep: self.input.pkg_dirs.fqn(cur.package).clone().into(),
                    });
                }

                link_core_deps.insert(cur);
                emitted.insert(cur);
                trace!(?cur, "Post-order: emitted");

                if self.build_env.target_backend().is_native() && !pkg.c_stub_files.is_empty() {
                    c_stub_deps.insert(cur.package);
                }
            }
        }

        Ok((link_core_deps, c_stub_deps, abort_overridden))
    }

    /// Propagate the link configuration of the packages in dependency to the output list
    fn propagate_link_config(
        &self,
        toolchain: &Toolchain,
        pkgs: impl Iterator<Item = PackageId>,
        out: &mut Vec<String>,
    ) {
        let Some(prebuild) = self.prebuild_config else {
            return;
        };
        let is_msvc_like = toolchain.uses_msvc_link_library_names();
        for pkg in pkgs {
            let Some(link_config) = prebuild.package_configs.get(&pkg) else {
                continue;
            };

            let link_flags = link_config
                .link_flags
                .as_ref()
                .and_then(|x| shlex::split(x));
            if let Some(link_flags) = link_flags {
                out.extend(link_flags);
            }

            for lib in &link_config.link_libs {
                if is_msvc_like {
                    out.push(format!("{lib}.lib"));
                } else {
                    out.push(format!("-l{lib}"));
                }
            }

            for path in &link_config.link_search_paths {
                if is_msvc_like {
                    out.push(format!("/LIBPATH:{path}"));
                } else {
                    out.push(format!("-L{path}"));
                }
            }
        }
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_bundle(
        &mut self,
        node: BuildPlanNode,
        module_id: ModuleId,
    ) -> Result<(), BuildPlanConstructError> {
        // Bundling a module gathers the build result of all its non-virtual packages, in topo order
        let topo_sorted_pkgs = self.topo_sort_module_packages(module_id);
        let mut bundle_targets = Vec::new();
        let target_backend = self.build_env.target_backend();
        for target in topo_sorted_pkgs.into_iter() {
            let pkg = self.input.pkg_dirs.get_package(target.package);
            if !pkg.effective_supported_targets.contains(&target_backend) {
                trace!(
                    ?module_id,
                    ?target,
                    ?target_backend,
                    "skipping bundle target that does not support backend"
                );
                continue;
            }
            if !pkg.has_implementation() {
                // TODO(bundle-virtual): Request the contract as a bundle root
                // when the stdlib gains a pure virtual package, then cover the
                // installed sidecar with a bundle integration fixture.
                trace!(
                    ?module_id,
                    ?target,
                    "skipping bundle target without implementation"
                );
                continue;
            }

            trace!(?module_id, ?target, "enqueuing bundle dependency");
            self.require_artifact(
                node,
                ArtifactKey::CoreIr {
                    package: target.package,
                    target_kind: target.kind,
                },
            );

            if pkg.is_virtual() {
                trace!(
                    ?module_id,
                    ?target,
                    "skipping including as build target for virtual package"
                );
                continue;
            }
            bundle_targets.push(target);
        }
        trace!(
            ?module_id,
            count = bundle_targets.len(),
            "recording bundle targets"
        );
        self.res
            .backend
            .bundle_info
            .insert(module_id, BuildBundleInfo { bundle_targets });
        self.resolved_node(node);

        Ok(())
    }

    /// List all packages in the module in topological order.
    ///
    /// This is a DFS that limits its traversal to only packages within the module.
    fn topo_sort_module_packages(&self, module_id: ModuleId) -> Vec<BuildTarget> {
        let pkg_map = self
            .input
            .pkg_dirs
            .packages_for_module(module_id)
            .expect("Must exist");

        let cmp_by_fqn = |a: &PackageId, b: &PackageId| {
            let pkg_a = self.input.pkg_dirs.get_package(*a);
            let pkg_b = self.input.pkg_dirs.get_package(*b);
            pkg_a.fqn.cmp(&pkg_b.fqn)
        };

        // Seed the DFS with packages sorted by FQN to ensure deterministic traversal.
        let mut seeds: Vec<_> = pkg_map.values().copied().collect();
        seeds.sort_by(cmp_by_fqn);

        let graph = &self.input.pkg_rel.dep_graph;
        let mut ordered = Vec::new();
        let mut visited = HashSet::new();
        let mut stack = Vec::new();

        for pkg_id in seeds {
            let target = pkg_id.build_target(TargetKind::Source);
            if visited.contains(&target) {
                continue;
            }

            // Classic iterative DFS with an explicit stack so we control ordering precisely.
            stack.push((target, false));
            while let Some((node, expanded)) = stack.pop() {
                if expanded {
                    let pkg = self.input.pkg_dirs.get_package(node.package);
                    if pkg.module == module_id {
                        ordered.push(node);
                    }
                    continue;
                }

                if !visited.insert(node) {
                    continue;
                }

                stack.push((node, true));

                let mut deps: Vec<_> = graph
                    .neighbors_directed(node, petgraph::Direction::Outgoing)
                    .filter(|dep| dep.kind == TargetKind::Source)
                    .filter(|dep| {
                        let pkg = self.input.pkg_dirs.get_package(dep.package);
                        pkg.module == module_id
                    })
                    .collect();

                // Visit dependencies in sorted order, pushing reverse so the smallest comes off first.
                deps.sort_by(|a, b| cmp_by_fqn(&a.package, &b.package));

                for dep in deps.into_iter().rev() {
                    if !visited.contains(&dep) {
                        stack.push((dep, false));
                    }
                }
            }
        }

        ordered
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_runtime_object(
        &mut self,
        node: BuildPlanNode,
        _index: u32,
    ) -> Result<(), BuildPlanConstructError> {
        self.need_node(node);
        self.resolved_node(node);
        Ok(())
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_runtime_lib(
        &mut self,
        node: BuildPlanNode,
    ) -> Result<(), BuildPlanConstructError> {
        let effective_native_toolchain = self.effective_native_toolchain(None).map_err(|e| {
            BuildPlanConstructError::FailedToSetRuntimeCC(self.new_native_linker_context(e))
        })?;
        let source_files = toolchain::runtime_source_paths()
            .map_err(BuildPlanConstructError::FailedToFindRuntimeSources)?;
        let builds_static_archive = self.build_env.tcc_run().is_none();
        let simdutf_objects = if builds_static_archive
            && self.build_env.opt_level == OptLevel::Release
            && effective_native_toolchain.cc().can_use_simdutf()
        {
            self.build_env
                .compiler_paths
                .as_ref()
                .expect("native build environment should include compiler paths")
                .simdutf_object_paths()
                .map(|objects| objects.into_iter().collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let static_archive_fingerprint = (builds_static_archive
            && effective_native_toolchain
                .cc()
                .archiver_updates_existing_archive())
        .then(|| runtime_archive_fingerprint(&source_files, &simdutf_objects));
        self.res.backend.runtime_info = Some(BuildRuntimeInfo {
            effective_native_toolchain,
            source_files,
            simdutf_objects,
            static_archive_fingerprint,
        });

        if builds_static_archive {
            let source_count = self
                .res
                .backend
                .runtime_info
                .as_ref()
                .expect("runtime info was just populated")
                .source_files
                .len();
            for index in 0..source_count {
                let source = &self
                    .res
                    .backend
                    .runtime_info
                    .as_ref()
                    .expect("runtime info was just populated")
                    .source_files[index];
                self.require_artifact(
                    node,
                    ArtifactKey::RuntimeObject {
                        source: runtime_source_key(source),
                    },
                );
            }
        }

        self.resolved_node(node);
        Ok(())
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_generate_mbti(
        &mut self,
        _node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        // Generate mbti relies on the `.mi` files spitted out by `moonc`, which
        // usually means `moonc check` instead of `moonc build`.
        self.check_backend_compatibility_for_mi_dep(_node, target)?;
        self.require_check_mi_of_dep(_node, target);
        self.resolved_node(_node);
        Ok(())
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_parse_mbti(
        &mut self,
        node: BuildPlanNode,
        target: PackageId,
    ) -> Result<(), BuildPlanConstructError> {
        // Parse MBTI depends on the .mi of its dependencies
        let pkg = self.input.pkg_dirs.get_package(target);

        assert!(
            pkg.is_virtual(),
            "Only virtual packages can have their .mi parsed from .mbti files"
        );

        // The virtual contract may itself be a declared package prebuild
        // output. Lowering records the selected `.mbti` path as an ordinary
        // file input, so n2 connects it to the matching prebuild provider.
        self.plan_package_prebuild(target)?;

        let canonical = pkg.root_path.join(MBTI_USER_WRITTEN);
        let legacy = pkg
            .root_path
            .join(format!("{}.mbti", pkg.fqn.short_alias()));
        let selected = [canonical, legacy].into_iter().find(|candidate| {
            pkg.virtual_mbti_files.contains(candidate)
                || self
                    .res
                    .package_prebuild
                    .output_paths(target)
                    .any(|output| output == candidate)
        });
        let Some(selected) = selected else {
            return Err(BuildPlanConstructError::MissingVirtualMbtiFile(
                pkg.fqn.clone().into(),
            ));
        };
        if selected.file_name().and_then(OsStr::to_str) != Some(MBTI_USER_WRITTEN) {
            self.user_log.warn(format!(
                "Using package name in MBTI file is deprecated. Please rename {} to {}",
                selected.display(),
                MBTI_USER_WRITTEN
            ));
        }
        self.res
            .backend
            .virtual_contract_inputs
            .insert(target, selected);

        for dep in self.input.pkg_rel.dep_graph.neighbors_directed(
            target.build_target(TargetKind::Source),
            petgraph::Direction::Outgoing,
        ) {
            self.check_backend_compatibility_for_mi_dep(node, dep)?;
            match self.build_env.action {
                RunMode::Check | RunMode::Prove => self.require_check_mi_of_dep(node, dep),
                RunMode::Build
                | RunMode::Run
                | RunMode::Test
                | RunMode::Bench
                | RunMode::Bundle => self.require_build_mi_of_dep(node, dep),
                RunMode::Format => {
                    unreachable!("format plans do not compile virtual package contracts")
                }
            }
        }

        self.resolved_node(node);

        Ok(())
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_build_docs(
        &mut self,
        node: BuildPlanNode,
        _module_id: ModuleId,
    ) -> Result<(), BuildPlanConstructError> {
        // For now, `moondoc` depends on *every check*, as specified in its
        // packages.json input. I guess bad things might happen if you don't?
        for (pkg_id, _) in self.input.pkg_dirs.all_packages(true) {
            self.require_artifact(
                node,
                ArtifactKey::CheckMi {
                    package: pkg_id,
                    target_kind: TargetKind::Source,
                },
            );
        }
        self.resolved_node(node);
        Ok(())
    }

    fn resolve_custom_prebuild(
        &self,
        package: PackageId,
        prebuild_cmd: &MoonPkgGenerate,
    ) -> Result<PrebuildInfo, BuildPlanConstructError> {
        let pkg = self.input.pkg_dirs.get_package(package);
        let module = &self.input.module_dirs[pkg.module];

        // Warn about suspicious outputs
        for output in prebuild_cmd.output().iter() {
            let output: &Path = output.as_ref();
            let Some(filename) = output.file_name().and_then(OsStr::to_str) else {
                continue;
            };

            // If the output is a moonbit source and it does not live in the current dir
            if (filename.ends_with(".mbt") || filename.ends_with(".mbt.md"))
                && output.parent() != Some("".as_ref())
            {
                self.user_log.warn(format!(
                    "Prebuild output '{}' is not in the package directory of package {}. \
                     Such behavior is not supported. \
                     The build system will not add it to the list of MoonBit files to compile. \
                     If you really intend to generate files for another package, \
                     please move the prebuild command to that package instead.",
                    output.display(),
                    pkg.fqn
                ));
            }
            // If the file looks like a package manifest
            if is_moon_mod(filename) || is_moon_pkg(filename) {
                self.user_log.warn(format!(
                    "Prebuild output '{}' of package {} looks like a package manifest file. \
                     Overwriting package manifests is not supported and may lead to unexpected behavior.",
                    output.display(),
                    pkg.fqn
                ));
            }
        }

        // Resolve declared paths once; the same outputs drive package-file
        // interpretation and lowering.
        let input_paths = prebuild_cmd
            .input()
            .iter()
            .map(|path| {
                let input_path = Path::new(path);
                if input_path.is_absolute() {
                    input_path.to_path_buf()
                } else {
                    RelativePath::new(path).normalize().to_path(&pkg.root_path)
                }
            })
            .collect::<Vec<_>>();
        let output_paths = prebuild_cmd
            .output()
            .iter()
            .map(|path| {
                let output_path = Path::new(path);
                if output_path.is_absolute() {
                    output_path.to_path_buf()
                } else {
                    RelativePath::new(path).normalize().to_path(&pkg.root_path)
                }
            })
            .collect::<Vec<_>>();
        let command_cwd = module.as_path();
        let command_input_paths = prebuild_command_paths(command_cwd, &input_paths);
        let command_output_paths = prebuild_command_paths(command_cwd, &output_paths);

        let command = match prebuild_cmd {
            MoonPkgGenerate::Direct { command, .. } => Cow::Borrowed(command.as_str()),
            MoonPkgGenerate::Rule { rule, .. } => {
                let module_info = self.input.module_info(pkg.module);
                Cow::Owned(resolve_prebuild_rule_command(
                    pkg.local_rules(),
                    module_info,
                    rule,
                    pkg.fqn.clone().into(),
                )?)
            }
        };

        let command = resolve_prebuild_command(
            &command,
            module,
            self.mooncake_bin_dir,
            &pkg.root_path,
            &command_input_paths,
            &command_output_paths,
        );

        Ok(PrebuildInfo {
            resolved_inputs: input_paths,
            resolved_outputs: output_paths,
            cwd: command_cwd.to_path_buf(),
            command,
        })
    }
}

fn resolve_prebuild_rule_command(
    local_rules: Option<&[MoonModRule]>,
    module: &MoonMod,
    rule_name: &str,
    package: PackageFQNWithSource,
) -> Result<String, BuildPlanConstructError> {
    if let Some(rules) = local_rules {
        for rule in rules {
            if rule.name == rule_name {
                return Ok(rule.command.clone());
            }
        }
    }
    if let Some(rules) = &module.rule {
        for rule in rules {
            if rule.name == rule_name {
                return Ok(rule.command.clone());
            }
        }
    }
    Err(BuildPlanConstructError::InvalidPrebuildRule {
        package,
        message: format!("Unknown dev_build rule `{}` in moon.pkg.", rule_name),
    })
}

/// Concatenate two optional strings
fn cat_opt(x: Option<String>, y: Option<&str>) -> Option<String> {
    match (x, y) {
        (Some(mut a), Some(b)) => {
            a.push_str(b);
            Some(a)
        }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b.to_string()),
        (None, None) => None,
    }
}

static PREBUILD_AUTOMATA: LazyLock<aho_corasick::AhoCorasick> = LazyLock::new(|| {
    aho_corasick::AhoCorasickBuilder::new()
        .build([MOONCAKE_BIN, MOD_DIR, PKG_DIR, "$input", "$output"])
        .expect("Failed to build automata")
});

/// Substitute prebuild command placeholders and resolve a relative shell argv0.
///
/// The `:embed ` discriminator remains intact so lowering can emit that built-in
/// invocation as structured argv instead of treating it as a shell program.
///
/// # Note about binary dependency artifacts
///
/// Currently, `moon` does not have direct support for referencing binary
/// dependency artifacts. Artifacts built by binary dependencies are placed in
/// either of these two locations:
///
/// - `<project target dir>/__moonbin__/[bin-target-name]`, if the artifact
///   comes from a regular dependency from the official registry.
/// - At the root of the corresponding module's source directory, if the
///   artifact comes from a local dependency.
///
/// In prebuild commands, users can reference the former location using
/// `$mooncake_bin`. For the latter case, users need to manually specify the
/// relative path from the module source directory. For robustness, the relative
/// path needs to be resolved against the module source directory itself before
/// executing.
///
/// Windows puts another issue on top of this: binary dependencies are
/// Powershell scripts appended with `.ps1` extension. Therefore, we need to
/// resolve `argv[0]` and append a `.ps1` if such file exists.
fn resolve_prebuild_command(
    command: &str,
    mod_source: &Path,
    mooncake_bin_dir: &Path,
    pkg_source: &Path,
    input_files: &[String],
    output_files: &[String],
) -> String {
    use std::fmt::Write;

    let mut resolved = String::new();

    // Perform replacements
    let mut last_end = 0usize;
    for magic in PREBUILD_AUTOMATA.find_iter(command) {
        // Commit previous segment
        if magic.start() > last_end {
            resolved.push_str(&command[last_end..magic.start()]);
        }

        // Insert replacement
        // See the IDs in CHECK_AUTOMATA
        match magic.pattern().as_usize() {
            // $mooncake_bin => <project target dir>/__moonbin__
            0 => {
                write!(resolved, "{}", mooncake_bin_dir.display()).expect("write can't fail");
            }
            // $mod_dir => <mod_source>
            1 => {
                write!(resolved, "{}", mod_source.display()).expect("write can't fail");
            }
            // $pkg_dir => <pkg_source>
            2 => {
                write!(resolved, "{}", pkg_source.display()).expect("write can't fail");
            }
            // $input => (existing)<input_1>, <input_2>, ...
            3 => {
                for (i, f) in input_files.iter().enumerate() {
                    if i != 0 {
                        write!(resolved, " ").expect("write can't fail");
                    }
                    write!(resolved, "{f}").expect("write can't fail");
                }
            }
            4 => {
                for (i, f) in output_files.iter().enumerate() {
                    if i != 0 {
                        write!(resolved, " ").expect("write can't fail");
                    }
                    write!(resolved, "{f}").expect("write can't fail");
                }
            }
            _ => unreachable!("Unexpected pattern id from CHECK_AUTOMATA"),
        }
        last_end = magic.end();
    }

    if last_end < command.len() {
        resolved.push_str(&command[last_end..]);
    }

    // Resolve argv[0]
    let argv0 = moonutil::shlex::get_argv0_native(&resolved);
    // Check if argv[0] looks like a relative path.
    let looks_like_path = argv0.contains(std::path::is_separator);
    let is_relative = looks_like_path && !Path::new(&argv0).is_absolute();
    // For relative paths, we need to resolve it against the package source
    // directory. Since we cannot easily splice the resolved path back into the
    // command string, we just prepend the source directory to the front of the
    // command.
    #[cfg(not(windows))]
    if is_relative {
        resolved = format!(
            "{}{}{}",
            mod_source.display(),
            std::path::MAIN_SEPARATOR,
            resolved
        );
    }
    // For windows, we also need to check if the resolved path with `.ps1` exists.
    #[cfg(windows)]
    if is_relative {
        let resolved_path_ps1 = dunce::canonicalize(mod_source.join(format!("{}.ps1", argv0)));
        if let Ok(new_argv0) = resolved_path_ps1
            && new_argv0.is_file()
        {
            use moonutil::shlex::split_argv0_windows;

            let (_argv0, rest) = split_argv0_windows(&resolved);
            // This is safe because '"' is not a valid path character on Windows,
            // and the original argv[0] must be a path-like string.
            resolved = format!("powershell \"{}\" {}", new_argv0.display(), rest);
        }
    }

    resolved
}

fn prebuild_command_path(cwd: &Path, path: &Path) -> String {
    match path.relative_to(cwd) {
        Ok(relative_path) => {
            let normalized = relative_path.normalize();
            if normalized.as_str().is_empty() {
                ".".to_string()
            } else {
                format!("./{normalized}")
            }
        }
        Err(_) => path.display().to_string(),
    }
}

fn prebuild_command_paths(cwd: &Path, paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| prebuild_command_path(cwd, path))
        .collect()
}

#[cfg(test)]
mod tests {
    use moonutil::compiler_flags::{ARKind, CCKind};

    use super::*;

    fn test_module(rules: Vec<moonutil::manifest::MoonModRule>) -> MoonMod {
        MoonMod {
            name: "test".to_string(),
            rule: Some(rules),
            ..Default::default()
        }
    }

    fn test_package(module: &MoonMod) -> PackageFQNWithSource {
        PackageFQNWithSource::new(
            moonutil::resolution::ModuleSource::from_local_module(
                module,
                std::path::Path::new("."),
            ),
            crate::pkg_name::PackagePath::new("").expect("empty package path should parse"),
        )
    }

    fn toolchain(target_triple: &str) -> Toolchain {
        Toolchain::from_path_probe(CC {
            cc_kind: CCKind::Clang,
            cc_path: "/toolchain/bin/clang".to_string(),
            ar_kind: ARKind::AppleLibtool,
            ar_path: "/toolchain/bin/libtool".to_string(),
            target_triple: Some(target_triple.to_string()),
            is_env_override: false,
        })
    }

    #[test]
    fn dsym_generation_policy_is_backend_specific() {
        let apple_toolchain = toolchain("arm64-apple-darwin");
        let linux_toolchain = toolchain("x86_64-unknown-linux-gnu");
        let direct_apple = DirectNativeMode::Target(NativeTarget::Aarch64AppleDarwin);

        assert!(should_generate_llvm_dsym(true, OperatingSystem::MacOS));
        assert!(!should_generate_llvm_dsym(true, OperatingSystem::Linux));
        assert!(!should_generate_llvm_dsym(false, OperatingSystem::MacOS));

        assert!(should_generate_direct_native_dsym(
            &direct_apple,
            true,
            &apple_toolchain
        ));
        assert!(!should_generate_direct_native_dsym(
            &direct_apple,
            false,
            &apple_toolchain
        ));
        assert!(!should_generate_direct_native_dsym(
            &direct_apple,
            true,
            &linux_toolchain
        ));
    }

    #[test]
    fn prebuild_command_path_is_relative_to_command_cwd() {
        assert_eq!(
            prebuild_command_path(Path::new("module"), Path::new("module/src/lib/input.txt")),
            "./src/lib/input.txt"
        );
    }

    #[test]
    fn prebuild_command_path_can_include_module_from_workspace_cwd() {
        assert_eq!(
            prebuild_command_path(
                Path::new("workspace"),
                Path::new("workspace/member/src/lib/input.txt")
            ),
            "./member/src/lib/input.txt"
        );
    }

    #[test]
    fn prebuild_command_path_normalizes_dot_segments() {
        assert_eq!(
            prebuild_command_path(
                Path::new("module"),
                Path::new("module/src/lib/../assets/./input.txt")
            ),
            "./src/assets/input.txt"
        );
    }

    #[test]
    fn prebuild_command_path_handles_absolute_paths() {
        assert_eq!(
            prebuild_command_path(
                Path::new("/module"),
                Path::new("/module/src/lib/../assets/input.txt")
            ),
            "./src/assets/input.txt"
        );
    }

    #[test]
    fn resolve_prebuild_command_uses_relative_input_and_output_placeholders() {
        let command = resolve_prebuild_command(
            "generate --inputs $input --outputs $output",
            Path::new("module"),
            Path::new("module/_build/__moonbin__"),
            Path::new("module/src/lib"),
            &prebuild_command_paths(
                Path::new("module"),
                &[
                    PathBuf::from("module/src/lib/input.txt"),
                    PathBuf::from("module/src/lib/../assets/second.txt"),
                ],
            ),
            &prebuild_command_paths(
                Path::new("module"),
                &[
                    PathBuf::from("module/src/lib/generated.mbt"),
                    PathBuf::from("module/src/lib/./generated_2.mbt"),
                ],
            ),
        );

        assert_eq!(
            command,
            "generate --inputs ./src/lib/input.txt ./src/assets/second.txt --outputs ./src/lib/generated.mbt ./src/lib/generated_2.mbt"
        );
    }

    #[test]
    fn resolve_prebuild_rule_command_uses_module_rule() {
        let module = test_module(vec![
            moonutil::manifest::MoonModRule {
                name: "rule1".to_string(),
                command: "exe1".to_string(),
            },
            moonutil::manifest::MoonModRule {
                name: "rule2".to_string(),
                command: "exe2".to_string(),
            },
        ]);

        assert_eq!(
            resolve_prebuild_rule_command(None, &module, "rule2", test_package(&module))
                .expect("rule2 should resolve"),
            "exe2"
        );
    }

    #[test]
    fn resolve_prebuild_rule_command_uses_package_local_rule() {
        let module = test_module(vec![moonutil::manifest::MoonModRule {
            name: "module_rule".to_string(),
            command: "module_exe".to_string(),
        }]);
        let local_rules = vec![moonutil::manifest::MoonModRule {
            name: "local_rule".to_string(),
            command: "local_exe".to_string(),
        }];

        assert_eq!(
            resolve_prebuild_rule_command(
                Some(&local_rules),
                &module,
                "local_rule",
                test_package(&module)
            )
            .expect("local rule should resolve"),
            "local_exe"
        );
    }

    #[test]
    fn resolve_prebuild_rule_command_prefers_package_local_rule() {
        let module = test_module(vec![moonutil::manifest::MoonModRule {
            name: "rule1".to_string(),
            command: "module_exe".to_string(),
        }]);
        let local_rules = vec![moonutil::manifest::MoonModRule {
            name: "rule1".to_string(),
            command: "local_exe".to_string(),
        }];

        assert_eq!(
            resolve_prebuild_rule_command(
                Some(&local_rules),
                &module,
                "rule1",
                test_package(&module)
            )
            .expect("local rule should shadow module rule"),
            "local_exe"
        );
    }

    #[test]
    fn resolve_prebuild_rule_command_rejects_unknown_rule() {
        let module = test_module(vec![moonutil::manifest::MoonModRule {
            name: "rule1".to_string(),
            command: "exe1".to_string(),
        }]);

        let err = resolve_prebuild_rule_command(None, &module, "missing", test_package(&module))
            .expect_err("missing rule should fail");
        assert!(err.to_string().contains("Unknown dev_build rule"));
    }
}
