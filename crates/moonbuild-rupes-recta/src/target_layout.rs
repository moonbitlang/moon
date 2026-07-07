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

//! Target directory layout and build artifact path resolution.
//!
//! [`TargetLayout`] owns paths under the selected target directory. It does not
//! know about installed toolchain artifacts. [`ArtifactPathResolver`] composes a
//! target layout with optional stdlib/toolchain artifacts for callers that need
//! to resolve logical build products to physical paths.

use std::{
    ffi::OsStr,
    fmt::Display,
    path::{Path, PathBuf},
};

use moonutil::{
    build_options::RunMode,
    cond_expr::OptLevel,
    mooncakes::{ModuleName, ModuleSource, result::ResolvedEnv},
    target::TargetBackend,
};

use crate::{
    ResolveOutput,
    build_action_plan::{BuildAction, BuildProduct},
    discover::{DiscoverResult, DiscoveredLocalProject},
    model::{BuildTarget, OperatingSystem, PackageId, RunBackend, TargetKind},
    pkg_name::PackageFQN,
};

/// The extension of the intermediate representation emitted by the Build action.
const CORE_EXTENSION: &str = ".core";
/// The extension of the package public interface file emitted by Check and Build.
const MI_EXTENSION: &str = ".mi";
/// Implementation packages generate a dummy mi file so they are not rebuilt every time.
const IMPL_MI_EXTENSION: &str = ".impl.mi";

#[derive(Clone, Copy, Debug)]
pub enum ExecutableArtifact {
    Wasm { use_wat: bool },
    WasmGC { use_wat: bool },
    Js,
    NativeExecutable,
    TccRunResponseFile,
    LlvmExecutable,
}

#[derive(Clone, Copy, Debug)]
pub enum LinkedCoreArtifact {
    Wasm { use_wat: bool },
    WasmGC { use_wat: bool },
    Js,
    NativeC,
    NativeObject { os: OperatingSystem },
    LlvmObject { os: OperatingSystem },
}

impl LinkedCoreArtifact {
    fn target_backend(self) -> TargetBackend {
        match self {
            Self::Wasm { .. } => TargetBackend::Wasm,
            Self::WasmGC { .. } => TargetBackend::WasmGC,
            Self::Js => TargetBackend::Js,
            Self::NativeC | Self::NativeObject { .. } => TargetBackend::Native,
            Self::LlvmObject { .. } => TargetBackend::LLVM,
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Wasm { use_wat } | Self::WasmGC { use_wat } if use_wat => ".wat",
            Self::Wasm { .. } | Self::WasmGC { .. } => ".wasm",
            Self::Js => ".js",
            Self::NativeC => ".c",
            Self::NativeObject { os } | Self::LlvmObject { os } => object_file_ext(os),
        }
    }
}

impl ExecutableArtifact {
    fn target_backend(self) -> TargetBackend {
        match self {
            Self::Wasm { .. } => TargetBackend::Wasm,
            Self::WasmGC { .. } => TargetBackend::WasmGC,
            Self::Js => TargetBackend::Js,
            Self::NativeExecutable | Self::TccRunResponseFile => TargetBackend::Native,
            Self::LlvmExecutable => TargetBackend::LLVM,
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Wasm { use_wat } | Self::WasmGC { use_wat } if use_wat => ".wat",
            Self::Wasm { .. } | Self::WasmGC { .. } => ".wasm",
            Self::Js => ".js",
            Self::NativeExecutable | Self::LlvmExecutable => ".exe",
            Self::TccRunResponseFile => ".rspfile",
        }
    }
}

/// Whether target artifacts use the single-module compatibility layout or the
/// module-qualified workspace layout.
#[derive(Clone, Debug)]
pub enum TargetLayoutMode {
    /// Flatten packages from the selected module and place dependencies under `.mooncakes`.
    Mono { main_module: ModuleSource },
    /// Qualify every package by module name.
    Workspace,
}

impl TargetLayoutMode {
    pub fn from_resolve_output(resolve_output: &ResolveOutput) -> Self {
        match resolve_output.local_modules() {
            &[module_id] => Self::Mono {
                main_module: resolve_output.module_rel.module_source(module_id).clone(),
            },
            _ => Self::Workspace,
        }
    }

    pub fn from_fmt_resolve_output(resolved: &DiscoveredLocalProject) -> Self {
        match resolved.root_module_ids.as_slice() {
            &[module_id] => Self::Mono {
                main_module: resolved.root_modules[module_id].source().clone(),
            },
            _ => Self::Workspace,
        }
    }
}

/// Target folder layout for generated artifacts.
#[derive(Clone, Debug)]
pub struct TargetLayout {
    /// The base target directory, usually `<project-root>/_build`.
    target_base_dir: PathBuf,
    mode: TargetLayoutMode,
    /// The optimization level, debug or release.
    opt_level: OptLevel,
    /// The operation done.
    run_mode: RunMode,
}

impl TargetLayout {
    pub fn new(
        target_base_dir: PathBuf,
        mode: TargetLayoutMode,
        opt_level: OptLevel,
        run_mode: RunMode,
    ) -> Self {
        Self {
            target_base_dir,
            mode,
            opt_level,
            run_mode,
        }
    }

    pub fn from_resolve_output(
        target_base_dir: PathBuf,
        resolve_output: &ResolveOutput,
        opt_level: OptLevel,
        run_mode: RunMode,
    ) -> Self {
        Self::new(
            target_base_dir,
            TargetLayoutMode::from_resolve_output(resolve_output),
            opt_level,
            run_mode,
        )
    }

    pub fn from_fmt_resolve_output(
        target_base_dir: PathBuf,
        resolved: &DiscoveredLocalProject,
        opt_level: OptLevel,
    ) -> Self {
        Self::new(
            target_base_dir,
            TargetLayoutMode::from_fmt_resolve_output(resolved),
            opt_level,
            RunMode::Format,
        )
    }

    pub fn target_base_dir(&self) -> &Path {
        &self.target_base_dir
    }

    /// Returns the directory the given package resides in.
    ///
    /// In the mono compatibility layout, packages from the selected main module
    /// are flattened to `_build/<backend>[/<opt_level>/build]/<...package>/`
    /// and all others go under
    /// `_build/<backend>[/<opt_level>/build]/.mooncakes/<...module>/<...package>`.
    ///
    /// In the workspace layout, all packages go under
    /// `_build/<backend>[/<opt_level>/build]/<...module>/<...package>`.
    pub fn package_dir(&self, pkg: &PackageFQN, backend: TargetBackend) -> PathBuf {
        let mut dir = self.run_mode_dir(backend);

        match &self.mode {
            TargetLayoutMode::Mono { main_module } if pkg.module() == main_module => {
                // no nested directory for the working module
            }
            TargetLayoutMode::Mono { .. } => {
                dir.push(moonutil::constants::DEP_PATH);
                dir.extend(pkg.module().name().segments());
            }
            TargetLayoutMode::Workspace => {
                dir.extend(pkg.module().name().segments());
            }
        }
        dir.extend(pkg.package().segments());

        dir
    }

    pub fn run_mode_dir(&self, backend: TargetBackend) -> PathBuf {
        let mut dir = self.target_base_dir.clone();
        self.push_opt_and_run_mode(backend, &mut dir);
        dir
    }

    fn push_opt_and_run_mode(&self, backend: TargetBackend, dir: &mut PathBuf) {
        push_backend(dir, backend);

        match self.opt_level {
            OptLevel::Release => dir.push("release"),
            OptLevel::Debug => dir.push("debug"),
        }
        dir.push(self.run_mode.to_dir_name());
    }

    pub fn core_of_build_target(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
    ) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(target.package).fqn;
        let mut base_dir = self.package_dir(pkg_fqn, backend);
        base_dir.push(format!(
            "{}{}",
            artifact(pkg_fqn, target.kind),
            CORE_EXTENSION
        ));
        base_dir
    }

    pub fn mi_of_build_target(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
    ) -> PathBuf {
        self.mi_of_build_target_aux(pkg_list, target, backend, false)
    }

    pub fn mi_of_build_target_impl_virtual(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
    ) -> PathBuf {
        self.mi_of_build_target_aux(pkg_list, target, backend, true)
    }

    fn mi_of_build_target_aux(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
        is_implementing_virtual: bool,
    ) -> PathBuf {
        if self.run_mode == RunMode::Prove {
            return self.emit_proof_mi_path(pkg_list, target);
        }

        let pkg_fqn = &pkg_list.get_package(target.package).fqn;
        let mut base_dir = self.package_dir(pkg_fqn, backend);
        base_dir.push(format!(
            "{}{}",
            artifact(pkg_fqn, target.kind),
            if is_implementing_virtual {
                IMPL_MI_EXTENSION
            } else {
                MI_EXTENSION
            }
        ));
        base_dir
    }

    /// For each backend/opt_level/run_mode, there's a copy of all_pkgs.json.
    pub fn all_pkgs_of_build_target(&self, backend: TargetBackend) -> PathBuf {
        let mut dir = self.run_mode_dir(backend);
        dir.push(crate::all_pkgs::ALL_PKGS_JSON);
        dir
    }

    pub fn linked_core_of_build_target(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        linked_core: LinkedCoreArtifact,
    ) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(target.package).fqn;
        let backend = linked_core.target_backend();
        let mut base_dir = self.package_dir(pkg_fqn, backend);
        if matches!(linked_core, LinkedCoreArtifact::NativeObject { .. }) {
            base_dir.push("__moonbit_link_core__");
        }
        base_dir.push(format!(
            "{}{}",
            artifact(pkg_fqn, target.kind),
            linked_core.extension()
        ));
        base_dir
    }

    pub fn executable_of_build_target(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        executable: ExecutableArtifact,
    ) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(target.package).fqn;
        let mut base_dir = self.package_dir(pkg_fqn, executable.target_backend());
        base_dir.push(format!(
            "{}{}",
            artifact(pkg_fqn, target.kind),
            executable.extension(),
        ));
        base_dir
    }

    pub fn generated_test_driver(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
    ) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(target.package).fqn;
        let mut base_dir = self.package_dir(pkg_fqn, backend);
        base_dir.push(format!(
            "__generated_driver_for{}.mbt",
            build_kind_suffix_filename(target.kind)
        ));
        base_dir
    }

    pub fn generated_test_driver_metadata(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
    ) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(target.package).fqn;
        let mut base_dir = self.package_dir(pkg_fqn, backend);
        base_dir.push(format!(
            "_{}_info.json",
            build_kind_suffix_filename(target.kind)
        ));
        base_dir
    }

    pub fn bundle_result_path(&self, backend: TargetBackend, module: &ModuleName) -> PathBuf {
        let mut result = self.run_mode_dir(backend);
        result.push(format!("{}.core", module.last_segment()));
        result
    }

    pub fn runtime_output_dir(&self, backend: RunBackend) -> PathBuf {
        match backend {
            RunBackend::WasmGC | RunBackend::Wasm | RunBackend::Js => {
                panic!("Runtime output path is not applicable for non-native backends")
            }
            RunBackend::Native | RunBackend::Llvm => self.run_mode_dir(backend.into()),
        }
    }

    pub fn runtime_output_path(
        &self,
        backend: RunBackend,
        use_tcc_run: bool,
        os: OperatingSystem,
    ) -> PathBuf {
        let mut result = self.runtime_output_dir(backend);
        match backend {
            RunBackend::WasmGC | RunBackend::Wasm | RunBackend::Js => {
                panic!("Runtime output path is not applicable for non-native backends")
            }
            RunBackend::Native if use_tcc_run => {
                result.push(format!("libruntime{}", dynamic_library_ext(os)))
            }
            RunBackend::Native | RunBackend::Llvm => {
                result.push(format!("runtime{}", object_file_ext(os)))
            }
        }
        result
    }

    /// The *artifact* of the format operation.
    ///
    /// At the time of writing, it should only be used as a stamp file to
    /// indicate that formatting has been done. However, due to the way
    /// `moonfmt` works, it actually produces a formatted copy of the input file
    /// at this path.
    pub fn format_artifact_path(&self, pkg: &PackageFQN, filename: &OsStr) -> PathBuf {
        let mut result = self.package_dir(pkg, TargetBackend::WasmGC);
        result.push(filename);
        result
    }

    pub fn format_root_artifact_path(&self, filename: &OsStr) -> PathBuf {
        let mut result = self.run_mode_dir(TargetBackend::WasmGC);
        result.push(filename);
        result
    }

    pub fn generated_mbti_path(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
    ) -> PathBuf {
        let pkg = &pkg_list.get_package(target.package).fqn;
        let mut base_dir = self.package_dir(pkg, backend);
        base_dir.push(format!("{}.mbti", artifact(pkg, target.kind)));
        base_dir
    }

    pub fn verif_root(&self) -> PathBuf {
        self.target_base_dir.join("verif")
    }

    pub fn verif_package_dir(&self, pkg_list: &DiscoverResult, target: &BuildTarget) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(target.package).fqn;
        let mut dir = self.verif_root();
        dir.extend(pkg_fqn.package().segments());
        dir
    }

    pub fn why3_config_path(&self) -> PathBuf {
        self.verif_root().join("why3.conf")
    }

    pub fn emit_proof_whyml_path(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
    ) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(target.package).fqn;
        let mut path = self.verif_package_dir(pkg_list, target);
        path.push(format!("{}.mlw", proof_artifact_stem(pkg_fqn)));
        path
    }

    pub fn emit_proof_mi_path(&self, pkg_list: &DiscoverResult, target: &BuildTarget) -> PathBuf {
        let mut path = self.emit_proof_whyml_path(pkg_list, target);
        path.set_extension("mi");
        path
    }

    pub fn prove_whyml_path(&self, pkg_list: &DiscoverResult, target: &BuildTarget) -> PathBuf {
        self.emit_proof_whyml_path(pkg_list, target)
    }

    pub fn prove_mi_path(&self, pkg_list: &DiscoverResult, target: &BuildTarget) -> PathBuf {
        self.emit_proof_mi_path(pkg_list, target)
    }

    pub fn prove_report_path(&self, pkg_list: &DiscoverResult, target: &BuildTarget) -> PathBuf {
        let mut dir = self.verif_package_dir(pkg_list, target);
        let pkg_fqn = &pkg_list.get_package(target.package).fqn;
        dir.push(format!("{}.proof.json", artifact(pkg_fqn, target.kind)));
        dir
    }

    /// Returns the path for a C stub object file.
    ///
    /// Format: `_build/{backend}/{opt_level}/build/{package_path}/{stub_name}.o`
    pub fn c_stub_object_path(
        &self,
        pkg_list: &DiscoverResult,
        package: PackageId,
        stub_name: &OsStr,
        backend: TargetBackend,
        os: OperatingSystem,
    ) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(package).fqn;
        let mut base_dir = self.package_dir(pkg_fqn, backend);
        let mut stub_name = stub_name.to_os_string();
        stub_name.push(object_file_ext(os));
        base_dir.push(stub_name);
        base_dir
    }

    pub fn c_stub_library_dir(
        &self,
        pkg_list: &DiscoverResult,
        package: PackageId,
        backend: TargetBackend,
    ) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(package).fqn;
        self.package_dir(pkg_fqn, backend)
    }

    /// Returns the path for a C stub static library archive.
    ///
    /// Format: `_build/{backend}/{opt_level}/build/{package_path}/lib{package_name}.a`
    pub fn c_stub_archive_path(
        &self,
        pkg_list: &DiscoverResult,
        package: PackageId,
        backend: TargetBackend,
        os: OperatingSystem,
    ) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(package).fqn;
        let mut base_dir = self.package_dir(pkg_fqn, backend);
        base_dir.push(format!(
            "lib{}{}",
            pkg_fqn.short_alias(),
            static_library_ext(os)
        ));
        base_dir
    }

    /// Returns the path for a C stub dynamic library.
    ///
    /// Format: `_build/{backend}/{opt_level}/build/{package_path}/lib{package_name}.{dylib_ext}`
    pub fn c_stub_link_dylib_path(
        &self,
        pkg_list: &DiscoverResult,
        package: PackageId,
        backend: TargetBackend,
        os: OperatingSystem,
    ) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(package).fqn;
        let mut base_dir = self.package_dir(pkg_fqn, backend);
        base_dir.push(format!(
            "lib{}{}",
            pkg_fqn.short_alias(),
            dynamic_library_ext(os)
        ));
        base_dir
    }

    /// Returns the directory for outputting documentation.
    ///
    /// Format: `_build/doc`
    pub fn doc_dir(&self) -> PathBuf {
        let mut dir = self.target_base_dir.clone();
        dir.push("doc");
        dir
    }

    /// Returns the path of `packages.json`, the metadata file to be read by
    /// IDE plugins and other tools.
    pub fn packages_json_path(&self) -> PathBuf {
        let mut path = self.target_base_dir.clone();
        path.push("packages.json");
        path
    }

    pub fn n2_db_path(&self, target_backend: TargetBackend) -> PathBuf {
        let mut path = self.run_mode_dir(target_backend);
        path.push(format!("{}.moon_db", self.run_mode.to_dir_name()));
        path
    }
}

#[derive(Clone, Debug)]
pub struct ArtifactPathResolver {
    target_layout: TargetLayout,
    stdlib_dir: Option<PathBuf>,
}

impl ArtifactPathResolver {
    pub fn new(target_layout: TargetLayout, stdlib_dir: Option<PathBuf>) -> Self {
        Self {
            target_layout,
            stdlib_dir,
        }
    }

    pub fn target_layout(&self) -> &TargetLayout {
        &self.target_layout
    }

    fn stdlib_dir(&self) -> Option<&Path> {
        self.stdlib_dir.as_deref()
    }

    pub fn core_of_build_target(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
    ) -> PathBuf {
        // Special case: `abort` lives in core.
        // Only redirect abort to prebuilt stdlib artifacts when stdlib is injected.
        if let Some(stdlib_dir) = self.stdlib_dir()
            && let Some(abort) = pkg_list.abort_pkg()
            && abort == target.package
        {
            if target.kind == TargetKind::Source {
                return moonutil::toolchain::abort_core_in(stdlib_dir, backend);
            } else {
                panic!("Cannot import `.mi` for moonbitlang/core/abort");
            }
        }

        self.target_layout
            .core_of_build_target(pkg_list, target, backend)
    }

    pub fn mi_of_build_target(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
    ) -> PathBuf {
        self.mi_of_build_target_aux(pkg_list, target, backend, false)
            .into_path()
    }

    pub fn mi_of_build_target_impl_virtual(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
    ) -> MiPathResult {
        self.mi_of_build_target_aux(pkg_list, target, backend, true)
    }

    fn mi_of_build_target_aux(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
        is_implementing_virtual: bool,
    ) -> MiPathResult {
        // Special case: `abort` lives in core.
        // Only redirect abort to prebuilt stdlib artifacts when stdlib is injected.
        if let Some(stdlib_dir) = self.stdlib_dir()
            && let Some(abort) = pkg_list.abort_pkg()
            && abort == target.package
        {
            if target.kind == TargetKind::Source {
                return MiPathResult::StdAbort(moonutil::toolchain::abort_mi_in(
                    stdlib_dir,
                    backend,
                    is_implementing_virtual,
                ));
            } else {
                panic!("Cannot import `.mi` for moonbitlang/core/abort");
            }
        }

        // Stdlib packages use prebuilt .mi only when stdlib is injected.
        if let Some(stdlib_dir) = self.stdlib_dir()
            && pkg_list.get_package(target.package).is_stdlib
        {
            return MiPathResult::Std(stdlib_mi_path(
                stdlib_dir,
                backend,
                &pkg_list.get_package(target.package).fqn,
            ));
        }

        MiPathResult::Regular(self.target_layout.mi_of_build_target_aux(
            pkg_list,
            target,
            backend,
            is_implementing_virtual,
        ))
    }

    pub(crate) fn paths_for_product(
        &self,
        product: &BuildProduct,
        action_context: BuildAction<'_>,
        packages: &DiscoverResult,
        modules: &ResolvedEnv,
        options: ArtifactPathOptions,
    ) -> Vec<PathBuf> {
        Self::assert_product_matches_action(product, action_context);

        match product {
            BuildProduct::PackageInterface { target } => {
                self.package_interface_paths(action_context, *target, packages, options)
            }
            BuildProduct::PackageCoreIr { target } => {
                vec![self.core_of_build_target(packages, target, options.target_backend.into())]
            }
            BuildProduct::ProofInterface { target } => match action_context {
                BuildAction::EmitProof { .. } => {
                    vec![self.target_layout.emit_proof_mi_path(packages, target)]
                }
                BuildAction::Prove { .. } => {
                    vec![self.target_layout.prove_mi_path(packages, target)]
                }
                _ => panic!("proof interface action context should be a proof action"),
            },
            BuildProduct::ProofWhyml { target } => match action_context {
                BuildAction::EmitProof { .. } => {
                    vec![self.target_layout.emit_proof_whyml_path(packages, target)]
                }
                BuildAction::Prove { .. } => {
                    vec![self.target_layout.prove_whyml_path(packages, target)]
                }
                _ => panic!("proof whyml action context should be a proof action"),
            },
            BuildProduct::ProofReport { target } => {
                vec![self.target_layout.prove_report_path(packages, target)]
            }
            BuildProduct::CStubObject { package, index } => {
                let pkg = packages.get_package(*package);
                let file_name = &pkg.c_stub_files[*index as usize];
                vec![
                    self.target_layout.c_stub_object_path(
                        packages,
                        *package,
                        file_name
                            .file_stem()
                            .expect("c stub file should have a file name"),
                        options.target_backend.into(),
                        options.os,
                    ),
                ]
            }
            BuildProduct::CStubLibrary { package } => {
                if options.use_tcc_run {
                    vec![self.target_layout.c_stub_link_dylib_path(
                        packages,
                        *package,
                        options.target_backend.into(),
                        options.os,
                    )]
                } else {
                    vec![self.target_layout.c_stub_archive_path(
                        packages,
                        *package,
                        options.target_backend.into(),
                        options.os,
                    )]
                }
            }
            BuildProduct::LinkedCore { target } => {
                vec![self.target_layout.linked_core_of_build_target(
                    packages,
                    target,
                    options.linked_core,
                )]
            }
            BuildProduct::Executable { target } => {
                vec![self.target_layout.executable_of_build_target(
                    packages,
                    target,
                    options.executable,
                )]
            }
            BuildProduct::GeneratedTestDriver { target } => {
                vec![self.target_layout.generated_test_driver(
                    packages,
                    target,
                    options.target_backend.into(),
                )]
            }
            BuildProduct::GeneratedTestMetadata { target } => {
                vec![self.target_layout.generated_test_driver_metadata(
                    packages,
                    target,
                    options.target_backend.into(),
                )]
            }
            BuildProduct::BundleResult { module } => {
                let module_name = modules.module_source(*module);
                vec![
                    self.target_layout
                        .bundle_result_path(options.target_backend.into(), module_name.name()),
                ]
            }
            BuildProduct::RuntimeLib => vec![self.target_layout.runtime_output_path(
                options.target_backend,
                options.use_tcc_run,
                options.os,
            )],
            BuildProduct::GeneratedMbti { target } => {
                vec![self.target_layout.generated_mbti_path(
                    packages,
                    target,
                    options.target_backend.into(),
                )]
            }
            BuildProduct::DocsDir => vec![self.target_layout.doc_dir()],
            BuildProduct::VirtualPackageInterface { package } => {
                let target = package.build_target(TargetKind::Source);
                vec![self.mi_of_build_target(packages, &target, options.target_backend.into())]
            }
            BuildProduct::MoonLexGeneratedSource { package, index } => {
                let pkg_info = packages.get_package(*package);
                let mbtlex_file = &pkg_info.mbt_lex_files[*index as usize];
                vec![mbtlex_file.with_extension("mbt")]
            }
            BuildProduct::MoonYaccGeneratedSource { package, index } => {
                let pkg_info = packages.get_package(*package);
                let mbtyacc_file = &pkg_info.mbt_yacc_files[*index as usize];
                vec![mbtyacc_file.with_extension("mbt")]
            }
            BuildProduct::PrebuildOutputPath { path } => vec![path.clone()],
        }
    }

    fn assert_product_matches_action(product: &BuildProduct, action_context: BuildAction<'_>) {
        let matches = match (product, action_context) {
            (
                BuildProduct::PackageInterface { target },
                BuildAction::Check {
                    target: action_target,
                    ..
                }
                | BuildAction::BuildCore {
                    target: action_target,
                    ..
                },
            ) => *target == action_target,
            (
                BuildProduct::PackageCoreIr { target },
                BuildAction::BuildCore {
                    target: action_target,
                    ..
                },
            ) => *target == action_target,
            (
                BuildProduct::ProofInterface { target } | BuildProduct::ProofWhyml { target },
                BuildAction::EmitProof {
                    target: action_target,
                    ..
                }
                | BuildAction::Prove {
                    target: action_target,
                    ..
                },
            ) => *target == action_target,
            (
                BuildProduct::ProofReport { target },
                BuildAction::Prove {
                    target: action_target,
                    ..
                },
            ) => *target == action_target,
            (
                BuildProduct::CStubObject { package, index },
                BuildAction::BuildCStub {
                    package: action_package,
                    index: action_index,
                    ..
                },
            ) => *package == action_package && *index == action_index,
            (
                BuildProduct::CStubLibrary { package },
                BuildAction::ArchiveOrLinkCStubs {
                    package: action_package,
                    ..
                },
            ) => *package == action_package,
            (
                BuildProduct::LinkedCore { target },
                BuildAction::LinkCore {
                    target: action_target,
                    ..
                },
            ) => *target == action_target,
            (
                BuildProduct::Executable { target },
                BuildAction::MakeExecutable {
                    target: action_target,
                    ..
                },
            ) => *target == action_target,
            (
                BuildProduct::GeneratedTestDriver { target }
                | BuildProduct::GeneratedTestMetadata { target },
                BuildAction::GenerateTestInfo {
                    target: action_target,
                    ..
                },
            ) => *target == action_target,
            (
                BuildProduct::BundleResult { module },
                BuildAction::Bundle {
                    module: action_module,
                    ..
                },
            ) => *module == action_module,
            (BuildProduct::RuntimeLib, BuildAction::BuildRuntimeLib { .. }) => true,
            (
                BuildProduct::GeneratedMbti { target },
                BuildAction::GenerateMbti {
                    target: action_target,
                },
            ) => *target == action_target,
            (BuildProduct::DocsDir, BuildAction::BuildDocs { .. }) => true,
            (
                BuildProduct::VirtualPackageInterface { package },
                BuildAction::BuildVirtual {
                    package: action_package,
                },
            ) => *package == action_package,
            (
                BuildProduct::MoonLexGeneratedSource { package, index },
                BuildAction::RunMoonLexPrebuild {
                    package: action_package,
                    index: action_index,
                },
            ) => *package == action_package && *index == action_index,
            (
                BuildProduct::MoonYaccGeneratedSource { package, index },
                BuildAction::RunMoonYaccPrebuild {
                    package: action_package,
                    index: action_index,
                },
            ) => *package == action_package && *index == action_index,
            (BuildProduct::PrebuildOutputPath { .. }, BuildAction::RunPrebuild { .. }) => true,
            _ => false,
        };
        assert!(
            matches,
            "build product should be resolved with matching action context: {product:?}, {action_context:?}"
        );
    }

    fn package_interface_paths(
        &self,
        action_context: BuildAction<'_>,
        target: BuildTarget,
        packages: &DiscoverResult,
        options: ArtifactPathOptions,
    ) -> Vec<PathBuf> {
        match action_context {
            BuildAction::Check { info, .. } if info.check_mi_against.is_some() => {
                // Generate a `.mi` artifact in edge cases such as --no-mi and
                // virtual implementation checks, but avoid declaring prebuilt
                // stdlib artifacts as outputs of the check action.
                match self.mi_of_build_target_impl_virtual(
                    packages,
                    &target,
                    options.target_backend.into(),
                ) {
                    MiPathResult::StdAbort(_) => Vec::new(),
                    MiPathResult::Std(p) => {
                        // This should not happen because there is no
                        // implementation package in stdlib other than abort.
                        tracing::warn!(
                            "stdlib mi should not be needed for check as an implementation package: {:?}",
                            p
                        );
                        Vec::new()
                    }
                    MiPathResult::Regular(p) => vec![p],
                }
            }
            BuildAction::Check { .. } | BuildAction::BuildCore { .. } => {
                vec![self.mi_of_build_target(packages, &target, options.target_backend.into())]
            }
            _ => panic!("package interface action context should be Check or BuildCore"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ArtifactPathOptions {
    pub target_backend: RunBackend,
    pub use_tcc_run: bool,
    pub os: OperatingSystem,
    pub executable: ExecutableArtifact,
    pub linked_core: LinkedCoreArtifact,
}

pub enum MiPathResult {
    Regular(PathBuf),
    StdAbort(PathBuf),
    Std(PathBuf),
}

impl MiPathResult {
    pub fn into_path(self) -> PathBuf {
        match self {
            MiPathResult::Regular(p) => p,
            MiPathResult::StdAbort(p) => p,
            MiPathResult::Std(p) => p,
        }
    }
}

/// A common structure for generating artifact basenames of packages.
///
/// We need to disambiguate between different kinds of output, so each artifact
/// will have a different suffix.
///
/// Note that this is different from [`crate::build_lower::compiler::CompiledPackageName`],
/// which represents the full package name passed to the compiler.
#[derive(Clone, Debug)]
struct PackageArtifactName<'a> {
    pub fqn: &'a PackageFQN,
    pub kind: TargetKind,
}

fn artifact(fqn: &'_ PackageFQN, kind: TargetKind) -> PackageArtifactName<'_> {
    PackageArtifactName { fqn, kind }
}

fn encode_proof_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        match byte {
            b'a'..=b'z' | b'0'..=b'9' => out.push(byte as char),
            b'_' => out.push_str("_u"),
            _ => out.push_str(&format!("_x{byte:02x}")),
        }
    }
    out
}

pub(crate) fn proof_artifact_stem(fqn: &PackageFQN) -> String {
    let mut stem = String::from("pkg");
    for segment in fqn.segments() {
        let encoded = encode_proof_segment(segment);
        stem.push('_');
        stem.push_str(&encoded.len().to_string());
        stem.push('_');
        stem.push_str(&encoded);
    }
    stem
}

impl Display for PackageArtifactName<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}",
            self.fqn.short_alias(),
            build_kind_suffix(self.kind)
        )
    }
}

fn push_backend(path: &mut PathBuf, backend: TargetBackend) {
    path.push(backend.to_dir_name())
}

fn build_kind_suffix(kind: TargetKind) -> &'static str {
    match kind {
        TargetKind::Source => "",
        TargetKind::WhiteboxTest => ".whitebox_test",
        TargetKind::BlackboxTest => ".blackbox_test",
        TargetKind::InlineTest => ".internal_test",
        TargetKind::SubPackage => "_sub",
    }
}

fn build_kind_suffix_filename(kind: TargetKind) -> &'static str {
    match kind {
        TargetKind::Source => "",
        TargetKind::WhiteboxTest => "_whitebox_test",
        TargetKind::BlackboxTest => "_blackbox_test",
        TargetKind::InlineTest => "_internal_test",
        TargetKind::SubPackage => "_sub",
    }
}

/// Returns the file extension for static libraries on the given OS.
fn static_library_ext(os: OperatingSystem) -> &'static str {
    match os {
        OperatingSystem::Windows => ".lib",
        OperatingSystem::Linux | OperatingSystem::MacOS => ".a",
        OperatingSystem::None => panic!("No static library extension for no-OS targets"),
    }
}

/// Returns the file extension for dynamic libraries on the given OS.
fn dynamic_library_ext(os: OperatingSystem) -> &'static str {
    match os {
        OperatingSystem::Windows => ".dll",
        OperatingSystem::Linux => ".so",
        OperatingSystem::MacOS => ".dylib",
        OperatingSystem::None => panic!("No dynamic library extension for no-OS targets"),
    }
}

/// Returns the file extension for object files on the given OS.
fn object_file_ext(os: OperatingSystem) -> &'static str {
    match os {
        OperatingSystem::Windows => ".obj",
        OperatingSystem::Linux | OperatingSystem::MacOS => ".o",
        OperatingSystem::None => panic!("No object file extension for no-OS targets"),
    }
}

pub fn stdlib_mi_path(core_root: &Path, backend: TargetBackend, fqn: &PackageFQN) -> PathBuf {
    let package_name = fqn.package().as_str();
    let package_last_segment = fqn
        .package()
        .segments()
        .next_back()
        .expect("Package must have at least one segment");
    moonutil::toolchain::core_package_mi_in(core_root, backend, package_name, package_last_segment)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use indexmap::IndexSet;
    use moonutil::mooncakes::{DEFAULT_VERSION, ModuleName, ModuleSource};
    use moonutil::{
        compiler_flags::{ARKind, CC, CCKind, Toolchain},
        module::MoonMod,
        package::{MoonPkg, MoonPkgFormatter, SupportedTargetsDeclKind},
    };

    use super::*;
    use crate::{
        build_action_plan::{BuildAction, BuildProduct},
        build_plan::{BuildCStubsInfo, BuildPlan, BuildRuntimeInfo, BuildTargetInfo, PrebuildInfo},
        discover::DiscoveredPackage,
        model::BuildPlanNode,
        pkg_name::{PackageFQN, PackagePath},
    };

    fn module(name: &str) -> ModuleSource {
        ModuleSource::local_path(
            name.parse::<ModuleName>()
                .expect("test module name should parse"),
            PathBuf::from(format!("/tmp/{name}")),
            DEFAULT_VERSION.clone(),
        )
    }

    fn layout(mode: TargetLayoutMode) -> TargetLayout {
        TargetLayout::new(
            PathBuf::from("_build"),
            mode,
            OptLevel::Debug,
            RunMode::Build,
        )
    }

    fn artifact_options(target_backend: RunBackend, use_tcc_run: bool) -> ArtifactPathOptions {
        ArtifactPathOptions {
            target_backend,
            use_tcc_run,
            os: OperatingSystem::Linux,
            executable: ExecutableArtifact::NativeExecutable,
            linked_core: LinkedCoreArtifact::NativeC,
        }
    }

    fn supported_targets() -> IndexSet<TargetBackend> {
        TargetBackend::all().iter().copied().collect()
    }

    fn moon_pkg(supported_targets: IndexSet<TargetBackend>) -> MoonPkg {
        MoonPkg {
            name: None,
            is_main: false,
            force_link: false,
            sub_package: None,
            imports: Vec::new(),
            wbtest_imports: Vec::new(),
            test_imports: Vec::new(),
            formatter: MoonPkgFormatter {
                ignore: Default::default(),
            },
            link: None,
            warn_list: None,
            proof_enabled: false,
            targets: None,
            pre_build: None,
            bin_name: None,
            bin_target: None,
            supported_targets,
            native_stub: None,
            virtual_pkg: None,
            implement: None,
            overrides: None,
            max_concurrent_tests: None,
            regex_backend: None,
            local_rules: None,
        }
    }

    fn moon_mod(name: &str) -> MoonMod {
        MoonMod {
            name: name.to_string(),
            version: None,
            deps: Default::default(),
            bin_deps: None,
            readme: None,
            repository: None,
            license: None,
            keywords: None,
            description: None,
            compile_flags: None,
            link_flags: None,
            checksum: None,
            source: None,
            rule: None,
            ext: Default::default(),
            warn_list: None,
            include: None,
            exclude: None,
            preferred_target: None,
            supported_targets: None,
            scripts: None,
            __moonbit_unstable_prebuild: None,
        }
    }

    fn build_target_info() -> BuildTargetInfo {
        BuildTargetInfo {
            regular_files: Vec::new(),
            mbtp_files: Vec::new(),
            whitebox_files: Vec::new(),
            doctest_files: Vec::new(),
            warn_list: None,
            specified_no_mi: false,
            patch_file: None,
            why3_config: None,
            check_mi_against: None,
            value_tracing: false,
        }
    }

    fn c_stubs_info() -> BuildCStubsInfo {
        BuildCStubsInfo {
            effective_native_toolchain: system_cc_toolchain(),
            cc_flags: Vec::new(),
            link_flags: Vec::new(),
        }
    }

    fn system_cc_toolchain() -> Toolchain {
        Toolchain::from_path_probe(CC {
            cc_kind: CCKind::SystemCC,
            cc_path: "cc".to_string(),
            ar_kind: ARKind::GnuAr,
            ar_path: "ar".to_string(),
            target_triple: None,
            is_env_override: false,
        })
    }

    fn runtime_info() -> BuildRuntimeInfo {
        BuildRuntimeInfo {
            effective_native_toolchain: system_cc_toolchain(),
        }
    }

    fn prebuild_info(path: PathBuf) -> PrebuildInfo {
        PrebuildInfo {
            resolved_inputs: Vec::new(),
            resolved_outputs: vec![path],
            cwd: PathBuf::from("."),
            command: "generate".to_string(),
        }
    }

    fn package_fixture(package_path: &str) -> (DiscoverResult, ResolvedEnv, PackageId) {
        let module_source = module("username/hello");
        let (modules, module_id) =
            ResolvedEnv::only_one_module(module_source.clone(), moon_mod("username/hello"));
        let package_path =
            PackagePath::new(package_path).expect("test package path should be valid");
        let supported_targets = supported_targets();
        let package = DiscoveredPackage {
            root_path: PathBuf::from(package_path.as_str()),
            module: module_id,
            fqn: PackageFQN::new(module_source, package_path.clone()),
            is_single_file: false,
            manifest_path: Some(PathBuf::from(package_path.as_str()).join("moon.pkg.json")),
            raw: Box::new(moon_pkg(supported_targets.clone())),
            supported_targets_decl: SupportedTargetsDeclKind::Omitted,
            effective_supported_targets: supported_targets,
            source_files: Vec::new(),
            mbt_lex_files: Vec::new(),
            mbt_yacc_files: Vec::new(),
            mbt_md_files: Vec::new(),
            mbtp_files: Vec::new(),
            c_stub_files: vec![PathBuf::from("native/stub.c")],
            virtual_mbti: None,
            is_stdlib: false,
        };

        let mut packages = DiscoverResult::default();
        let package_id = packages.test_add_package(module_id, package_path, package);
        (packages, modules, package_id)
    }

    #[test]
    fn mono_layout_flattens_main_module_packages() {
        let main_module = module("username/hello");
        let dep_module = module("username/world");
        let layout = layout(TargetLayoutMode::Mono {
            main_module: main_module.clone(),
        });

        let main_pkg = PackageFQN::new(
            main_module,
            "lib"
                .parse::<PackagePath>()
                .expect("test package path should parse"),
        );
        let dep_pkg = PackageFQN::new(
            dep_module,
            "lib"
                .parse::<PackagePath>()
                .expect("test package path should parse"),
        );

        assert_eq!(
            layout.package_dir(&main_pkg, TargetBackend::WasmGC),
            PathBuf::from("_build/wasm-gc/debug/build/lib"),
        );
        assert_eq!(
            layout.package_dir(&dep_pkg, TargetBackend::WasmGC),
            PathBuf::from("_build/wasm-gc/debug/build/.mooncakes/username/world/lib"),
        );
    }

    #[test]
    fn workspace_layout_qualifies_all_packages_by_module() {
        let root_module = module("username/hello");
        let dep_module = module("username/world/v2");
        let layout = layout(TargetLayoutMode::Workspace);

        let root_pkg = PackageFQN::new(
            root_module,
            "lib"
                .parse::<PackagePath>()
                .expect("test package path should parse"),
        );
        let dep_pkg = PackageFQN::new(
            dep_module,
            "lib"
                .parse::<PackagePath>()
                .expect("test package path should parse"),
        );

        assert_eq!(
            layout.package_dir(&root_pkg, TargetBackend::WasmGC),
            PathBuf::from("_build/wasm-gc/debug/build/username/hello/lib"),
        );
        assert_eq!(
            layout.package_dir(&dep_pkg, TargetBackend::WasmGC),
            PathBuf::from("_build/wasm-gc/debug/build/username/world/v2/lib"),
        );
    }

    #[test]
    fn artifact_resolver_resolves_check_package_interface_product() {
        let (packages, modules, package) = package_fixture("ffi");
        let resolver = ArtifactPathResolver::new(
            layout(TargetLayoutMode::Mono {
                main_module: modules.module_source(modules.input_module_ids()[0]).clone(),
            }),
            None,
        );
        let target = package.build_target(TargetKind::Source);
        let node = BuildPlanNode::Check(target);
        let mut build_plan = BuildPlan::default();
        build_plan.test_add_node(node);
        build_plan.test_insert_build_target_info(target, build_target_info());
        let action_plan = build_plan.build_action_plan();
        let action_id = action_plan
            .action_ids()
            .next()
            .expect("test plan should have an action");

        assert_eq!(
            resolver.paths_for_product(
                &BuildProduct::PackageInterface { target },
                action_plan.action(action_id),
                &packages,
                &modules,
                artifact_options(RunBackend::WasmGC, false),
            ),
            vec![PathBuf::from("_build/wasm-gc/debug/build/ffi/ffi.mi")],
        );
    }

    #[test]
    fn artifact_resolver_resolves_build_core_package_interface_product() {
        let (packages, modules, package) = package_fixture("ffi");
        let resolver = ArtifactPathResolver::new(
            layout(TargetLayoutMode::Mono {
                main_module: modules.module_source(modules.input_module_ids()[0]).clone(),
            }),
            None,
        );
        let target = package.build_target(TargetKind::Source);
        let info = build_target_info();

        assert_eq!(
            resolver.paths_for_product(
                &BuildProduct::PackageInterface { target },
                BuildAction::BuildCore {
                    target,
                    info: &info,
                },
                &packages,
                &modules,
                artifact_options(RunBackend::WasmGC, false),
            ),
            vec![PathBuf::from("_build/wasm-gc/debug/build/ffi/ffi.mi")],
        );
    }

    #[test]
    fn artifact_resolver_resolves_impl_check_package_interface_product() {
        let (packages, modules, package) = package_fixture("ffi");
        let resolver = ArtifactPathResolver::new(
            layout(TargetLayoutMode::Mono {
                main_module: modules.module_source(modules.input_module_ids()[0]).clone(),
            }),
            None,
        );
        let target = package.build_target(TargetKind::Source);
        let mut info = build_target_info();
        info.check_mi_against = Some(target);

        assert_eq!(
            resolver.paths_for_product(
                &BuildProduct::PackageInterface { target },
                BuildAction::Check {
                    target,
                    info: &info,
                },
                &packages,
                &modules,
                artifact_options(RunBackend::WasmGC, false),
            ),
            vec![PathBuf::from("_build/wasm-gc/debug/build/ffi/ffi.impl.mi")],
        );
    }

    #[test]
    fn artifact_resolver_resolves_proof_products_with_matching_context() {
        let (packages, modules, package) = package_fixture("ffi");
        let resolver = ArtifactPathResolver::new(layout(TargetLayoutMode::Workspace), None);
        let target = package.build_target(TargetKind::Source);
        let info = build_target_info();

        assert_eq!(
            resolver.paths_for_product(
                &BuildProduct::ProofWhyml { target },
                BuildAction::EmitProof {
                    target,
                    info: &info,
                },
                &packages,
                &modules,
                artifact_options(RunBackend::WasmGC, false),
            ),
            vec![
                resolver
                    .target_layout
                    .emit_proof_whyml_path(&packages, &target)
            ],
        );
        assert_eq!(
            resolver.paths_for_product(
                &BuildProduct::ProofReport { target },
                BuildAction::Prove {
                    target,
                    info: &info,
                },
                &packages,
                &modules,
                artifact_options(RunBackend::WasmGC, false),
            ),
            vec![resolver.target_layout.prove_report_path(&packages, &target)],
        );
    }

    #[test]
    #[should_panic(expected = "build product should be resolved with matching action context")]
    fn artifact_resolver_rejects_mismatched_product_action_context() {
        let (packages, modules, package) = package_fixture("ffi");
        let resolver = ArtifactPathResolver::new(layout(TargetLayoutMode::Workspace), None);
        let target = package.build_target(TargetKind::Source);
        let info = build_target_info();

        let _ = resolver.paths_for_product(
            &BuildProduct::PackageCoreIr { target },
            BuildAction::Check {
                target,
                info: &info,
            },
            &packages,
            &modules,
            artifact_options(RunBackend::WasmGC, false),
        );
    }

    #[test]
    fn artifact_resolver_resolves_c_stub_library_artifacts() {
        let (packages, modules, package) = package_fixture("ffi");
        let resolver = ArtifactPathResolver::new(
            layout(TargetLayoutMode::Mono {
                main_module: modules.module_source(modules.input_module_ids()[0]).clone(),
            }),
            None,
        );
        let info = c_stubs_info();
        assert_eq!(
            resolver.paths_for_product(
                &BuildProduct::CStubLibrary { package },
                BuildAction::ArchiveOrLinkCStubs {
                    package,
                    info: &info,
                },
                &packages,
                &modules,
                artifact_options(RunBackend::Native, false),
            ),
            vec![PathBuf::from("_build/native/debug/build/ffi/libffi.a")],
        );
        assert_eq!(
            resolver.paths_for_product(
                &BuildProduct::CStubLibrary { package },
                BuildAction::ArchiveOrLinkCStubs {
                    package,
                    info: &info,
                },
                &packages,
                &modules,
                artifact_options(RunBackend::Native, true),
            ),
            vec![PathBuf::from("_build/native/debug/build/ffi/libffi.so")],
        );
    }

    #[test]
    fn artifact_resolver_handles_non_package_products() {
        let resolver = ArtifactPathResolver::new(layout(TargetLayoutMode::Workspace), None);
        let (packages, modules, package) = package_fixture("ffi");
        let module = modules.input_module_ids()[0];
        let prebuild_output = PathBuf::from("source/generated.mbt");
        let prebuild = prebuild_info(prebuild_output.clone());
        let options = ArtifactPathOptions {
            target_backend: RunBackend::Native,
            use_tcc_run: true,
            os: OperatingSystem::Linux,
            executable: ExecutableArtifact::TccRunResponseFile,
            linked_core: LinkedCoreArtifact::NativeC,
        };

        assert_eq!(
            resolver.paths_for_product(
                &BuildProduct::RuntimeLib,
                BuildAction::BuildRuntimeLib {
                    info: &runtime_info(),
                },
                &packages,
                &modules,
                options,
            ),
            vec![PathBuf::from("_build/native/debug/build/libruntime.so")],
        );
        assert_eq!(
            resolver.paths_for_product(
                &BuildProduct::DocsDir,
                BuildAction::BuildDocs { module },
                &packages,
                &modules,
                options,
            ),
            vec![PathBuf::from("_build/doc")],
        );
        assert_eq!(
            resolver.paths_for_product(
                &BuildProduct::PrebuildOutputPath {
                    path: prebuild_output.clone(),
                },
                BuildAction::RunPrebuild {
                    package,
                    index: 0,
                    info: &prebuild,
                },
                &packages,
                &modules,
                options,
            ),
            vec![PathBuf::from("source/generated.mbt")],
        );
    }

    #[test]
    fn native_executable_artifacts_keep_stable_suffix() {
        assert_eq!(ExecutableArtifact::NativeExecutable.extension(), ".exe");
        assert_eq!(ExecutableArtifact::LlvmExecutable.extension(), ".exe");
    }

    #[test]
    fn fmt_n2_db_path_uses_selected_profile() {
        let layout = TargetLayout::new(
            PathBuf::from("_build"),
            TargetLayoutMode::Workspace,
            OptLevel::Debug,
            RunMode::Format,
        );

        assert_eq!(
            layout.n2_db_path(TargetBackend::WasmGC),
            PathBuf::from("_build/wasm-gc/debug/format/format.moon_db"),
        );
    }

    #[test]
    fn proof_artifact_stem_is_stable_and_distinct() {
        let module = module("username/hello");
        let root_pkg = PackageFQN::new(module.clone(), PackagePath::empty());
        let nested_pkg = PackageFQN::new(
            module,
            "dep/internal_name"
                .parse::<PackagePath>()
                .expect("test package path should parse"),
        );

        assert_eq!(proof_artifact_stem(&root_pkg), "pkg_8_username_5_hello");
        assert_eq!(
            proof_artifact_stem(&nested_pkg),
            "pkg_8_username_5_hello_3_dep_14_internal_uname"
        );
        assert_ne!(
            proof_artifact_stem(&root_pkg),
            proof_artifact_stem(&nested_pkg)
        );
    }
}
