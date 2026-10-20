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
    compiler_flags::{self, CC, NativeLinkArg, OptLevel as CCOptLevel, Toolchain, ToolchainSource},
    cond_expr::OptLevel,
    constants::{
        MBTI_USER_WRITTEN, MOD_DIR, PKG_DIR, PackageSourceFileKind, is_moon_mod, is_moon_pkg,
        package_source_file_kind,
    },
    manifest::{MoonMod, MoonModRule},
    package::{MoonPkgGenerate, SupportedTargetsDeclKind},
    resolution::ModuleId,
    scripts::{IgnoredMoonScript, is_moon_script_ignored},
    toolchain,
};
use regex::Regex;
use relative_path::PathExt;
#[cfg(not(windows))]
use relative_path::RelativePath;
use tracing::{Level, debug, instrument, trace, warn};

use crate::{
    build_plan::{BuildBundleInfo, PrebuildInfo},
    cond_comp,
    discover::DiscoveredPackage,
    model::{
        BackendConfig, BuildPlanNode, BuildTarget, DebugSymbols, DirectNativeMode,
        NativeBackendMode, NativeTarget, OperatingSystem, PackageId, TargetKind,
    },
    pkg_name::PackageFQNWithSource,
};

use super::{
    BuildCStubsInfo, BuildPlanConstructError, BuildRuntimeInfo, BuildTargetInfo,
    CompileGeneratedCInfo, ForeignLibraryInfo, LinkCoreInfo, MakeExecutableInfo,
    PackagePrebuildKey,
    artifact::{ArtifactKey, package_file_key, runtime_source_key},
    constructor::{BuildPlanConstructor, PackageFileSet},
};

static BUILD_VAR_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$\{build\.([a-zA-Z0-9_]+)\}").expect("invalid build var regex"));
const PROOF_ENABLED_WARN_SUPPRESSIONS: &str = "-1-2-3-29";

/// A package's `link.native` compiler selection and the settings for
/// compiling its generated-C payload.
struct PackageNativeCompile {
    effective_native_toolchain: Toolchain,
    c_flags: Vec<String>,
    c_debug_info: bool,
    c_opt_level: CCOptLevel,
    native_allocator: compiler_flags::NativeAllocator,
}

fn should_generate_llvm_dsym(debug_symbols: bool, os: OperatingSystem) -> bool {
    debug_symbols && os == OperatingSystem::MacOS
}

fn should_generate_direct_native_dsym(
    mode: &DirectNativeMode,
    source_info: bool,
    toolchain: &Toolchain,
) -> bool {
    source_info
        && mode.target() == NativeTarget::Aarch64AppleDarwin
        && toolchain.cc().targets_apple_darwin()
}

impl<'a> BuildPlanConstructor<'a> {
    fn new_native_linker_context(&self, err: anyhow::Error) -> anyhow::Error {
        if self.res.backend.direct_native_target() == Some(NativeTarget::X86_64PcWindowsMsvc) {
            err.context(
                "Windows MSVC direct object native target requires an MSVC compiler/linker driver such as cl.exe or clang-cl.exe",
            )
        } else if self.res.backend.direct_native_target().is_some() {
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
            "MOON_CC is ignored for Windows MSVC direct object native target because it is not a cl-compatible driver; set MOON_CC to cl.exe or clang-cl.exe to override MSVC discovery.",
        );
    }

    fn effective_native_toolchain(&mut self, package_cc: Option<&CC>) -> anyhow::Result<Toolchain> {
        // TODO: Consume explicit toolchain candidates and environment overrides
        // once native toolchain discovery moves to command orchestration.
        debug_assert!(self.config.backend.target_backend().is_native());
        if self.res.backend.direct_native_target() == Some(NativeTarget::X86_64PcWindowsMsvc) {
            self.warn_incompatible_windows_msvc_env_override();
            return compiler_flags::windows_msvc_native_toolchain(package_cc);
        }

        compiler_flags::effective_native_toolchain(package_cc)
    }

    pub(super) fn warn_moon_cc_overrides(&self) {
        if !self.config.backend.target_backend().is_native() {
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
                    let m_name = self.input.module_graph.module_source(module);
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
        if !self.input.local_modules().contains(&pkg.module) {
            return Ok(());
        }

        let custom_rules = pkg.raw.pre_build.as_deref().unwrap_or_default();

        if !is_moon_script_ignored(IgnoredMoonScript::Prebuild) {
            let custom_actions = custom_rules
                .iter()
                .enumerate()
                .filter(|(index, _)| {
                    let key = PackagePrebuildKey {
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

        Ok(())
    }

    fn check_backend_compatibility_for_dep(
        &mut self,
        importer_target: BuildTarget,
        dep: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        let selected_backend = self.config.backend.target_backend();
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
            .rel
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
        if self.config.stdlib_path.is_some() && self.input.pkg_dirs.is_stdlib_package(dep.package) {
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
        if self.config.stdlib_path.is_some() && self.input.pkg_dirs.is_stdlib_package(dep.package) {
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
        if self.config.stdlib_path.is_some() && self.input.pkg_dirs.is_stdlib_package(dep.package) {
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
        if self.config.stdlib_path.is_some() && self.input.pkg_dirs.is_stdlib_package(dep.package) {
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
        if self.config.stdlib_path.is_some() && self.input.pkg_dirs.is_stdlib_package(dep.package) {
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
        if pkg.is_virtual() && !(self.config.stdlib_path.is_some() && pkg.is_stdlib) {
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
        if let Some(vpkg_id) = self.rel.virt_impl.get(target.package)
            && !(self.config.stdlib_path.is_some()
                && self.input.pkg_dirs.is_stdlib_package(*vpkg_id))
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
            .rel
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
            .rel
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
            .rel
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
            self.config.opt_level,
            self.config.backend.target_backend(),
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
                self.config.action,
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
                self.config.action,
                moonutil::build_options::RunMode::Check | moonutil::build_options::RunMode::Prove
            ))
        .then_some(PROOF_ENABLED_WARN_SUPPRESSIONS);
        let package_warn_list = cat_opt(pkg.raw.warn_list.clone(), proof_warn_list);
        let warn_list = cat_opt(
            cat_opt(module.warn_list.clone(), package_warn_list.as_deref()),
            self.config.warn_list.as_deref(),
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
            } else if let Some(implement) = self.rel.virt_impl.get(target.package) {
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
        target: PackageId,
        _index: u32,
    ) -> Result<(), BuildPlanConstructError> {
        self.populate_c_stubs_info(target)?;
        self.resolved_node(node);
        Ok(())
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_archive_or_link_c_stubs(
        &mut self,
        node: BuildPlanNode,
        target: PackageId,
    ) -> Result<(), BuildPlanConstructError> {
        self.require_c_stub_objects(node, target);
        self.populate_c_stubs_info(target)?;
        self.resolved_node(node);
        Ok(())
    }

    /// Require every C stub object declared by `package`, in declaration order.
    fn require_c_stub_objects(&mut self, consumer: BuildPlanNode, package: PackageId) {
        let pkg = self.input.pkg_dirs.get_package(package);
        for source in &pkg.c_stub_files {
            self.require_artifact(
                consumer,
                ArtifactKey::CStubObject {
                    package,
                    source: package_file_key(&pkg.root_path, source),
                },
            );
        }
    }

    /// Resolve the compile and archive settings shared by a package's C stub
    /// actions. Every C stub action populates it, so no action depends on
    /// another having been planned first.
    fn populate_c_stubs_info(&mut self, target: PackageId) -> Result<(), BuildPlanConstructError> {
        if self.res.backend.c_stubs_info.contains_key(&target) {
            return Ok(());
        }

        let pkg = self.input.pkg_dirs.get_package(target);
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

        let mut link_args = native_config
            .and_then(|native| native.stub_cc_link_flags.as_ref())
            .map(|s| self.replace_build_vars(target, pkg.module, s))
            .map(|replaced| {
                shlex::split(replaced.as_ref()).ok_or_else(|| {
                    BuildPlanConstructError::MalformedStubCCLinkFlags(pkg.fqn.clone().into())
                })
            })
            .transpose()?
            .unwrap_or_default()
            .into_iter()
            .map(NativeLinkArg::Opaque)
            .collect();

        let effective_native_toolchain = self
            .effective_native_toolchain(stub_cc.as_ref())
            .map_err(|e| {
                BuildPlanConstructError::FailedToSetStubCC(
                    self.new_native_linker_context(e),
                    pkg.fqn.clone().into(),
                )
            })?;

        if let Some(prebuild) = self.prebuild_config {
            prebuild.append_link_args(std::iter::once(target), &mut link_args);
        }

        // Backtrace-only requests concern MoonBit source locations. They must
        // not change the debug or optimization settings of user-written C stubs.
        let debug_info = self.config.debug_info.symbols == DebugSymbols::Full;
        // Preserve the existing C-stub profile policy: explicit debug info uses
        // debugging-friendly optimization, including with --release --no-strip.
        let opt_level = match (self.config.opt_level, debug_info) {
            (_, true) => CCOptLevel::Debug,
            (OptLevel::Release, false) => CCOptLevel::Speed,
            (OptLevel::Debug, false) => CCOptLevel::None,
        };
        let c_info = BuildCStubsInfo {
            effective_native_toolchain,
            debug_info,
            opt_level,
            cc_flags,
            link_args,
        };
        self.res.backend.c_stubs_info.insert(target, c_info);
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
        let (link_core_deps, _, link_stdlib_abort) = self.dfs_link_core_sources(target)?;

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
            link_stdlib_abort,
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
        debug_assert!(self.config.backend.target_backend().is_native());

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

        let PackageNativeCompile {
            effective_native_toolchain,
            c_flags,
            c_debug_info,
            c_opt_level,
            native_allocator,
        } = self.resolve_package_native_compile(target.package)?;

        let pkg = self.input.pkg_dirs.get_package(target.package);
        let native_config = pkg.raw.link.as_ref().and_then(|x| x.native.as_ref());
        let mut link_args = native_config
            .and_then(|native| native.cc_link_flags.as_ref())
            .map(|s| self.replace_build_vars(target.package, pkg.module, s))
            .map(|replaced| {
                shlex::split(replaced.as_ref()).ok_or_else(|| {
                    BuildPlanConstructError::MalformedCCLinkFlags(pkg.fqn.clone().into())
                })
            })
            .transpose()?
            .unwrap_or_default()
            .into_iter()
            .map(NativeLinkArg::Opaque)
            .collect();

        let mut link_pkgs: Vec<PackageId> = targets.iter().map(|x| x.package).collect();
        if !link_pkgs.contains(&target.package) {
            link_pkgs.push(target.package);
        }

        if let Some(prebuild) = self.prebuild_config {
            prebuild.append_link_args(link_pkgs.into_iter(), &mut link_args);
        }

        let moonc_debug_info = self.res.backend.moonc_debug_info();
        let generate_dsym = match &self.config.backend {
            BackendConfig::Llvm { os, .. } => should_generate_llvm_dsym(moonc_debug_info, *os),
            BackendConfig::Native { .. } => match self.res.backend.native_mode() {
                NativeBackendMode::DirectObject(mode) => should_generate_direct_native_dsym(
                    mode,
                    moonc_debug_info || self.config.debug_info.runtime_backtrace,
                    &effective_native_toolchain,
                ),
                NativeBackendMode::GeneratedC => false,
            },
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
            c_debug_info,
            c_opt_level,
            c_flags,
            link_args,
            native_allocator,
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

    /// Resolve the package's `link.native` compiler selection together with
    /// the settings for compiling its generated-C payload.
    fn resolve_package_native_compile(
        &mut self,
        package: PackageId,
    ) -> Result<PackageNativeCompile, BuildPlanConstructError> {
        let pkg = self.input.pkg_dirs.get_package(package);
        let native_config = pkg.raw.link.as_ref().and_then(|x| x.native.as_ref());
        let cc = native_config
            .and_then(|native| native.cc.as_ref())
            .map(|s| self.replace_build_vars(package, pkg.module, s))
            .map(|replaced| {
                CC::try_from_path(replaced.as_ref())
                    .map_err(|e| BuildPlanConstructError::FailedToSetCC(e, pkg.fqn.clone().into()))
            })
            .transpose()?;
        let c_flags = native_config
            .and_then(|native| native.cc_flags.as_ref())
            .map(|s| self.replace_build_vars(package, pkg.module, s))
            .map(|replaced| {
                shlex::split(replaced.as_ref()).ok_or_else(|| {
                    BuildPlanConstructError::MalformedCCFlags(pkg.fqn.clone().into())
                })
            })
            .transpose()?
            .unwrap_or_default();

        let effective_native_toolchain =
            self.effective_native_toolchain(cc.as_ref()).map_err(|e| {
                BuildPlanConstructError::FailedToSetCC(
                    self.new_native_linker_context(e),
                    pkg.fqn.clone().into(),
                )
            })?;

        let native_allocator = self
            .config
            .backend
            .native_allocator()
            .expect("native payload planning requires an allocator");
        native_allocator
            .validate_for_cc(effective_native_toolchain.cc())
            .map_err(BuildPlanConstructError::InvalidNativeAllocator)?;

        Ok(PackageNativeCompile {
            effective_native_toolchain,
            c_flags,
            // The native compiler must retain the source locations emitted into
            // generated C. A direct-object payload cannot create that information.
            c_debug_info: self.res.backend.direct_native_target().is_none()
                && self.config.debug_info.symbols != DebugSymbols::None,
            c_opt_level: match self.config.opt_level {
                OptLevel::Debug => CCOptLevel::Debug,
                OptLevel::Release => CCOptLevel::Speed,
            },
            native_allocator,
        })
    }

    /// Plan the native compiler action that turns generated-C linked Core
    /// output into an object.
    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_compile_generated_c(
        &mut self,
        node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        debug_assert!(matches!(
            self.res.backend.native_mode(),
            NativeBackendMode::GeneratedC
        ));

        self.require_artifact(
            node,
            ArtifactKey::LinkedCore {
                package: target.package,
                target_kind: target.kind,
            },
        );

        let PackageNativeCompile {
            effective_native_toolchain,
            c_flags,
            c_debug_info,
            c_opt_level,
            native_allocator,
        } = self.resolve_package_native_compile(target.package)?;
        self.res.backend.compile_generated_c_info.insert(
            target,
            CompileGeneratedCInfo {
                effective_native_toolchain,
                c_debug_info,
                c_opt_level,
                c_flags,
                native_allocator,
            },
        );

        self.resolved_node(node);
        Ok(())
    }

    /// Plan the static archive a foreign host links to use a foreign-library
    /// package on Native or LLVM.
    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_foreign_library(
        &mut self,
        node: BuildPlanNode,
        package: PackageId,
    ) -> Result<(), BuildPlanConstructError> {
        debug_assert!(self.config.backend.target_backend().is_native());

        // A foreign library exports its source target; tests link executables.
        let target = package.build_target(TargetKind::Source);
        let (_, c_stub_packages, _) = self.dfs_link_core_sources(target)?;

        // Archive members are objects. Generated C needs its own compile step,
        // while direct-object and LLVM linking already produce an object.
        let generated_c = matches!(&self.config.backend, BackendConfig::Native { .. })
            && matches!(
                self.res.backend.native_mode(),
                NativeBackendMode::GeneratedC
            );
        let payload = if generated_c {
            ArtifactKey::GeneratedCObject {
                package,
                target_kind: target.kind,
            }
        } else {
            ArtifactKey::LinkedCore {
                package,
                target_kind: target.kind,
            }
        };
        self.require_artifact(node, payload);

        // Static archives cannot nest, so C stubs and the runtime contribute
        // their objects directly instead of their intermediate archives.
        for c_stub_package in &c_stub_packages {
            self.require_c_stub_objects(node, *c_stub_package);
        }
        self.require_runtime_objects(node)?;
        // TODO: Report `link.native.cc-link-flags`, prebuild link arguments,
        // system libraries, and the backtrace library to the foreign host. A
        // static archive cannot carry them.

        let PackageNativeCompile {
            effective_native_toolchain,
            native_allocator,
            ..
        } = self.resolve_package_native_compile(package)?;
        self.res.backend.foreign_library_info.insert(
            package,
            ForeignLibraryInfo {
                effective_native_toolchain,
                c_stub_packages: c_stub_packages.into_iter().collect(),
                native_allocator,
            },
        );

        self.resolved_node(node);
        Ok(())
    }

    fn dfs_link_core_sources(
        &self,
        target: BuildTarget,
    ) -> Result<(IndexSet<BuildTarget>, IndexSet<PackageId>, bool), BuildPlanConstructError> {
        // This DFS is shared by both LinkCore and MakeExecutable actions.
        let vp_info = self.rel.virtual_users.get(target.package);

        let abort = if self.config.stdlib_path.is_some() {
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
        // Core versions without abort need no separate default implementation.
        let link_stdlib_abort = abort.is_some() && abort_override_pkg.is_none();

        let graph = &self.rel.dep_graph;

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
                        self.config.stdlib_path.is_none()
                            || !self.input.pkg_dirs.is_stdlib_package(dep.package)
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
                        if self.config.backend.target_backend().is_native()
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

                if self.config.backend.target_backend().is_native() && !pkg.c_stub_files.is_empty()
                {
                    c_stub_deps.insert(cur.package);
                }
            }
        }

        Ok((link_core_deps, c_stub_deps, link_stdlib_abort))
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
        let target_backend = self.config.backend.target_backend();
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

        let graph = &self.rel.dep_graph;
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
        self.require_runtime_objects(node)?;
        self.resolved_node(node);
        Ok(())
    }

    /// Require every runtime translation unit, resolving the runtime settings
    /// that their providers are lowered from.
    fn require_runtime_objects(
        &mut self,
        consumer: BuildPlanNode,
    ) -> Result<(), BuildPlanConstructError> {
        self.populate_runtime_info()?;
        let sources = self
            .res
            .backend
            .runtime_info
            .as_ref()
            .expect("runtime info was just populated")
            .source_files
            .iter()
            .map(|source| runtime_source_key(source))
            .collect::<Vec<_>>();
        for source in sources {
            self.require_artifact(consumer, ArtifactKey::RuntimeObject { source });
        }
        Ok(())
    }

    fn populate_runtime_info(&mut self) -> Result<(), BuildPlanConstructError> {
        if self.res.backend.runtime_info.is_some() {
            return Ok(());
        }

        let effective_native_toolchain = self.effective_native_toolchain(None).map_err(|e| {
            BuildPlanConstructError::FailedToSetRuntimeCC(self.new_native_linker_context(e))
        })?;
        // TODO: Consume supplied runtime sources and optional SIMDUTF objects
        // once command orchestration captures the runtime file inventory.
        let source_files = toolchain::runtime_source_paths()
            .map_err(BuildPlanConstructError::FailedToFindRuntimeSources)?;
        let simdutf_objects = if self.config.opt_level == OptLevel::Release
            && effective_native_toolchain.cc().can_use_simdutf()
        {
            self.config
                .backend
                .compiler_paths()
                .expect("native runtime planning requires compiler paths")
                .simdutf_object_paths()
                .map(|objects| objects.into_iter().collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let native_allocator = self
            .config
            .backend
            .native_allocator()
            .expect("native runtime planning requires an allocator");
        native_allocator
            .validate_for_cc(effective_native_toolchain.cc())
            .map_err(BuildPlanConstructError::InvalidNativeAllocator)?;

        // Runtime backtraces need the prebuilt host `libbacktrace.a`.
        let enable_backtrace = self.config.debug_info.runtime_backtrace
            && self.config.backend.os() != OperatingSystem::Windows
            && effective_native_toolchain.cc().targets_host();
        self.res.backend.runtime_info = Some(BuildRuntimeInfo {
            effective_native_toolchain,
            enable_backtrace,
            source_files,
            simdutf_objects,
            native_allocator,
        });
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

        for dep in self.rel.dep_graph.neighbors_directed(
            target.build_target(TargetKind::Source),
            petgraph::Direction::Outgoing,
        ) {
            self.check_backend_compatibility_for_mi_dep(node, dep)?;
            match self.config.action {
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
        module_id: ModuleId,
    ) -> Result<(), BuildPlanConstructError> {
        // Documentation starts from backend-compatible packages in the
        // selected module. Their Check MI requirements expand the same
        // dependency closure used by ordinary package checks.
        let target_backend = self.config.backend.target_backend();
        let packages = self
            .input
            .pkg_dirs
            .packages_for_module(module_id)
            .expect("documentation module must exist in the resolved package set");
        let supported_packages = packages
            .values()
            .copied()
            .filter(|&pkg_id| {
                self.input
                    .pkg_dirs
                    .get_package(pkg_id)
                    .effective_supported_targets
                    .contains(&target_backend)
            })
            .collect::<Vec<_>>();

        for pkg_id in supported_packages {
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

        // Resolve declared paths once; both graph tracking and file
        // interpretation must use the same spelling as source discovery.
        let input_paths = prebuild_cmd
            .input()
            .iter()
            .map(|path| resolve_prebuild_path(&pkg.root_path, path))
            .collect::<Vec<_>>();
        let output_paths = prebuild_cmd
            .output()
            .iter()
            .map(|path| resolve_prebuild_path(&pkg.root_path, path))
            .collect::<Vec<_>>();

        // Warn about suspicious outputs
        for (declared, output) in prebuild_cmd.output().iter().zip(&output_paths) {
            let Some(filename) = output.file_name().and_then(OsStr::to_str) else {
                continue;
            };

            // If the output is a moonbit source and it does not live in the current dir
            if (filename.ends_with(".mbt") || filename.ends_with(".mbt.md"))
                && output.parent() != Some(pkg.root_path.as_path())
            {
                self.user_log.warn(format!(
                    "Prebuild output '{}' is not in the package directory of package {}. \
                     Such behavior is not supported. \
                     The build system will not add it to the list of MoonBit files to compile. \
                     If you really intend to generate files for another package, \
                     please move the prebuild command to that package instead.",
                    declared, pkg.fqn
                ));
            }
            // If the file looks like a package manifest
            if is_moon_mod(filename) || is_moon_pkg(filename) {
                self.user_log.warn(format!(
                    "Prebuild output '{}' of package {} looks like a package manifest file. \
                     Overwriting package manifests is not supported and may lead to unexpected behavior.",
                    declared,
                    pkg.fqn
                ));
            }
        }

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

/// Substitute prebuild command placeholders and resolve a relative shell argv0.
///
/// The `:embed ` discriminator remains intact so lowering can emit that built-in
/// invocation as structured argv instead of treating it as a shell program.
///
/// Relative program paths are resolved against the module root. On Windows,
/// a matching PowerShell launcher is invoked through PowerShell explicitly.
fn resolve_prebuild_command(
    command: &str,
    mod_source: &Path,
    pkg_source: &Path,
    input_files: &[String],
    output_files: &[String],
) -> String {
    // Command templates are arbitrary text. Simplify short Windows roots before
    // users append their own path syntax, but do not interpret that syntax here.
    let mod_dir = dunce::simplified(mod_source).to_string_lossy();
    let pkg_dir = dunce::simplified(pkg_source).to_string_lossy();
    let inputs = input_files.join(" ");
    let outputs = output_files.join(" ");
    let replacements = [
        (MOD_DIR, mod_dir.as_ref()),
        (PKG_DIR, pkg_dir.as_ref()),
        ("$input", inputs.as_str()),
        ("$output", outputs.as_str()),
    ];
    let mut resolved = String::new();
    let mut last_end = 0;
    // Expand only the original template: directory names may contain literal
    // placeholder text, which must not be expanded a second time.
    while let Some(next_dollar) = command[last_end..].find('$') {
        let start = last_end + next_dollar;
        let Some((pattern, replacement)) = replacements
            .iter()
            .find(|(pattern, _)| command[start..].starts_with(pattern))
        else {
            resolved.push_str(&command[last_end..=start]);
            last_end = start + 1;
            continue;
        };
        resolved.push_str(&command[last_end..start]);
        resolved.push_str(replacement);
        last_end = start + pattern.len();
    }
    resolved.push_str(&command[last_end..]);

    // Resolve argv[0]
    let argv0 = moonutil::shlex::get_argv0_native(&resolved);
    // Check if argv[0] looks like a relative path.
    let looks_like_path = argv0.contains(std::path::is_separator);
    let is_relative = looks_like_path && !Path::new(&argv0).is_absolute();
    // For relative paths, we need to resolve it against the module source
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

fn resolve_prebuild_path(package_root: &Path, path: &str) -> PathBuf {
    // Construct declared paths in the same form as discovered package files,
    // before they enter the plan. Outputs may not exist yet, so resolve their
    // components lexically against the canonical package root.
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};

        let path = package_root.join(path);
        let mut components = path.components();
        let Some(Component::Prefix(prefix)) = components.next() else {
            return path.components().collect();
        };
        let root = match prefix.kind() {
            Prefix::Disk(drive) if path.is_absolute() => {
                PathBuf::from(format!(r"\\?\{}:", char::from(drive)))
            }
            Prefix::UNC(server, share) => {
                // A UNC prefix must be complete before native joining parses it.
                let mut root = std::ffi::OsString::from(r"\\?\UNC\");
                root.push(server);
                root.push(r"\");
                root.push(share);
                PathBuf::from(root)
            }
            _ => PathBuf::from(prefix.as_os_str()),
        };
        // An absolute declaration may have replaced the package root with an
        // ordinary drive/UNC root. Restore its verbatim spelling, then let
        // native joining resolve separators and parent components.
        root.join(components.as_path())
    }
    #[cfg(not(windows))]
    {
        let root = if Path::new(path).is_absolute() {
            Path::new("/")
        } else {
            package_root
        };
        RelativePath::new(path)
            .to_logical_path(root)
            .components()
            .collect()
    }
}

fn prebuild_command_path(cwd: &Path, path: &Path) -> String {
    // Preserve matching canonical roots: a long child may keep its prefix when
    // its short cwd can be simplified. Only reconcile spellings on mismatch.
    match path
        .relative_to(cwd)
        .or_else(|_| dunce::simplified(path).relative_to(dunce::simplified(cwd)))
    {
        Ok(relative_path) => {
            let normalized = relative_path.normalize();
            if normalized.as_str().is_empty() {
                ".".to_string()
            } else {
                format!("./{normalized}")
            }
        }
        Err(_) => dunce::simplified(path).display().to_string(),
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
            )
            .expect("test module should have a valid version"),
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
    #[cfg(windows)]
    fn prebuild_paths_use_native_verbatim_joining() {
        for (input_prefix, prefix) in [
            (r"C:", r"\\?\C:"),
            (r"\\?\C:", r"\\?\C:"),
            (r"\\server\share", r"\\?\UNC\server\share"),
            (r"\\?\UNC\server\share", r"\\?\UNC\server\share"),
        ] {
            let root = PathBuf::from(format!(r"{input_prefix}\project\pkg"));
            for (relative, expected) in [
                ("../a/b/c", r"\project\a\b\c"),
                ("../assets/./file", r"\project\assets\file"),
                (r"..\assets\.\file", r"\project\assets\file"),
                (r"../assets\..\file", r"\project\file"),
                ("../../../../file", r"\file"),
                ("../assets/nonexistent", r"\project\assets\nonexistent"),
            ] {
                let path = resolve_prebuild_path(&root, relative);
                assert_eq!(
                    path.as_os_str(),
                    Path::new(&format!("{prefix}{expected}")).as_os_str(),
                    "root={root:?}, relative={relative:?}",
                );
                assert!(!path.to_str().expect("UTF-8 fixture path").contains('/'));
                assert!(!path.components().any(|component| matches!(
                    component,
                    std::path::Component::CurDir | std::path::Component::ParentDir
                )));
            }
        }
        for (absolute, expected) in [
            (r"D:\assets\..\generated.mbt", r"\\?\D:\generated.mbt"),
            (
                r"\\server\share\assets\..\generated.mbt",
                r"\\?\UNC\server\share\generated.mbt",
            ),
        ] {
            assert_eq!(
                resolve_prebuild_path(Path::new(r"\\?\C:\project\pkg"), absolute).as_os_str(),
                Path::new(expected).as_os_str(),
            );
        }
    }

    #[test]
    fn prebuild_declarations_construct_the_package_file_spelling() {
        let package = if cfg!(windows) {
            Path::new(r"\\?\C:\project\pkg")
        } else {
            Path::new("/project/pkg")
        };
        // Discovery derives file paths from this same package root. No file
        // needs to exist, and no later comparison needs to reconcile spellings.
        let discovered = package.join("generated.mbt");
        let absolute = dunce::simplified(&discovered)
            .to_str()
            .expect("test path is UTF-8");
        for declared in [
            "generated.mbt",
            "./generated.mbt",
            "../pkg/generated.mbt",
            absolute,
        ] {
            assert_eq!(
                resolve_prebuild_path(package, declared).as_os_str(),
                discovered.as_os_str(),
                "declaration={declared:?}",
            );
        }
    }

    #[test]
    #[cfg(windows)]
    fn prebuild_command_keeps_textual_windows_paths() {
        let module = Path::new(r"\\?\C:\Users\user'quote\project $input with spaces");
        let package = module.join("src/app");
        let mut snapshot = String::new();
        for command in [
            r#":embed --text -i "$pkg_dir/../assets 'quoted'/./input.txt" -o "$pkg_dir/../app/generated.mbt""#,
            r#"tool --assets="$mod_dir/src/app/../assets" "$pkg_dir_extra/keep" "$input" "$output" $unknown"#,
            r#""$mod_dir/bin/tool.exe" "$pkg_dir\..\assets/../input.txt" "$pkg_dir/""#,
            r#"tool "-I$pkg_dir/include" "/I$pkg_dir/include" --mapping="$pkg_dir/include,public" "$pkg_dir/$output""#,
        ] {
            let resolved = resolve_prebuild_command(
                command,
                module,
                &package,
                &["./first.txt".to_owned(), "./second.txt".to_owned()],
                &["./generated.mbt".to_owned()],
            );
            snapshot.push_str(&resolved);
            snapshot.push('\n');
        }
        expect_test::expect_file!["snapshots/windows_prebuild_paths.snap"].assert_eq(&snapshot);
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

    #[cfg(windows)]
    #[test]
    fn prebuild_command_path_handles_windows_prefix_spellings() {
        for cwd in [r"C:\project", r"\\?\C:\project"] {
            for path in [r"C:\project\input.txt", r"\\?\C:\project\input.txt"] {
                assert_eq!(
                    prebuild_command_path(Path::new(cwd), Path::new(path)),
                    "./input.txt",
                    "cwd={cwd}, path={path}"
                );
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn prebuild_command_path_preserves_matching_roots_when_only_one_path_simplifies() {
        let cwd = Path::new(r"\\?\C:\project");
        let path = cwd.join("segment\\".repeat(40)).join("input.txt");
        assert_ne!(dunce::simplified(cwd), cwd);
        assert_eq!(dunce::simplified(&path), path);
        assert_eq!(
            prebuild_command_path(cwd, &path),
            format!("./{}input.txt", "segment/".repeat(40))
        );
        // Reverse the direction: the cwd is long, but the target is short.
        assert_eq!(
            prebuild_command_path(
                path.parent().expect("fixture file has a parent directory"),
                &cwd.join("output.txt"),
            ),
            format!("./{}output.txt", "../".repeat(40))
        );
    }

    #[cfg(windows)]
    #[test]
    fn prebuild_command_path_keeps_safe_absolute_fallbacks() {
        let path = Path::new(r"\\?\C:\project")
            .join("segment\\".repeat(40))
            .join("input.txt");
        // The short ordinary cwd cannot be reconciled by removing the long
        // path's required prefix. Fall back to the original usable spelling.
        assert_eq!(
            prebuild_command_path(Path::new(r"C:\project"), &path),
            path.display().to_string()
        );
        assert_eq!(
            prebuild_command_path(Path::new(r"C:\project"), Path::new(r"\\?\D:\input.txt")),
            r"D:\input.txt"
        );
        let unc = Path::new(r"\\?\UNC\server\share\project");
        assert_eq!(
            prebuild_command_path(unc, &unc.join("input.txt")),
            "./input.txt"
        );
    }

    #[test]
    fn resolve_prebuild_command_uses_relative_input_and_output_placeholders() {
        let command = resolve_prebuild_command(
            "generate --inputs $input --outputs $output",
            Path::new("module"),
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
    fn resolve_prebuild_command_preserves_attached_placeholders() {
        let command = resolve_prebuild_command(
            r#"tool "-I$pkg_dir/include" "/I$pkg_dir/include" --mapping="$pkg_dir/include,public" "$pkg_dir-suffix" $unknown"#,
            Path::new("module"),
            Path::new("module/pkg"),
            &[],
            &[],
        );
        expect_test::expect![[r#"tool "-Imodule/pkg/include" "/Imodule/pkg/include" --mapping="module/pkg/include,public" "module/pkg-suffix" $unknown"#]]
            .assert_eq(&command);
    }

    #[test]
    fn resolve_prebuild_command_replaces_all_placeholders_and_preserves_unknown_ones() {
        let command = resolve_prebuild_command(
            "tool $mod_dir $pkg_dir $input $output $unknown",
            Path::new("module"),
            Path::new("package"),
            &["input-a".to_owned(), "input-b".to_owned()],
            &["output".to_owned()],
        );

        assert_eq!(
            command,
            "tool module package input-a input-b output $unknown"
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
