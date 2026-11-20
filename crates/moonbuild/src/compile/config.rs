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

//! Resolve build environment inputs into the existing compile configuration.

use moonbuild_rupes_recta::{
    CompileConfig, ResolvedProject, RunMode,
    build_lower::WarningCondition,
    model::{
        BackendConfig, DebugInfoRequest, ENV_MOONBIT_NEW_NATIVE, NativeTarget, OperatingSystem,
    },
    target_layout::{ArtifactPathResolver, TargetLayout, stdlib_core_path, stdlib_mi_path},
};
use moonutil::{
    compiler_flags, cond_expr::OptLevel as BuildProfile, resolution::ModuleSourceKind,
    target::TargetBackend,
};
use std::path::Path;

/// Capture only the environment inputs used by the selected backend.
pub fn prepare_backend(
    target_backend: TargetBackend,
    output_wat: bool,
    wasi_link: bool,
) -> anyhow::Result<BackendConfig> {
    let collect_ref_cycle = std::env::var("MOON_COLLECT_REF_CYCLE").as_deref() == Ok("1");
    Ok(match target_backend {
        TargetBackend::Wasm => BackendConfig::Wasm {
            use_wat: output_wat,
            wasi_link: wasi_link && std::env::var("MOON_WASI_LINK").as_deref() != Ok("0"),
            new_allocator: std::env::var("MOON_WASM_NEW_ALLOCATOR").as_deref() == Ok("1"),
            collect_ref_cycle,
            separate_compilation: std::env::var("MOON_SEPARATE_COMPILATION").as_deref() == Ok("1"),
        },
        TargetBackend::WasmGC => BackendConfig::WasmGc {
            use_wat: output_wat,
        },
        TargetBackend::Js => BackendConfig::Js,
        TargetBackend::Native => {
            let new_native_env = std::env::var(ENV_MOONBIT_NEW_NATIVE).ok();
            BackendConfig::Native {
                direct_object_candidate: NativeTarget::from_host_with_new_native_env(
                    std::env::consts::ARCH,
                    std::env::consts::OS,
                    new_native_env.as_deref(),
                ),
                allocator: compiler_flags::NativeAllocator::from_env()?,
                collect_ref_cycle,
                os: std::env::consts::OS
                    .parse::<OperatingSystem>()
                    .expect("Unknown"),
                compiler_paths: compiler_flags::CompilerPaths::from_moon_dirs(),
            }
        }
        TargetBackend::LLVM => BackendConfig::Llvm {
            allocator: compiler_flags::NativeAllocator::from_env()?,
            os: std::env::consts::OS
                .parse::<OperatingSystem>()
                .expect("Unknown"),
            compiler_paths: compiler_flags::CompilerPaths::from_moon_dirs(),
        },
    })
}

/// Prepare paths and backend prerequisites for a resolved project.
/// Command-specific compiler options can be set on the returned configuration.
pub fn prepare_config(
    resolved_project: &ResolvedProject,
    target_dir: &Path,
    backend: BackendConfig,
    opt_level: BuildProfile,
    action: RunMode,
    debug_info: DebugInfoRequest,
) -> CompileConfig {
    let target_backend = backend.target_backend();
    // Module preparation has already selected the stdlib used for discovery.
    // Reuse its root for artifacts; a local core project has no injected stdlib.
    let modules = &resolved_project.module_graph;
    let stdlib_path = modules.stdlib().map(|id| {
        let ModuleSourceKind::Stdlib(path) = modules.module_source(id).source() else {
            unreachable!("injected stdlib must have a stdlib module source");
        };
        path.clone()
    });
    // Select one coherent artifact set for this backend. A partial bundle
    // must not mix installed interfaces with freshly compiled dependencies.
    // RR then provides the required interfaces using the normal dependency
    // layout, without writing into the toolchain or observing the filesystem.
    let stdlib_path = stdlib_path.filter(|root| {
        resolved_project
            .pkg_dirs
            .all_packages(false)
            .filter(|(_, pkg)| {
                pkg.is_stdlib && pkg.effective_supported_targets.contains(&target_backend)
            })
            .all(|(_, pkg)| {
                (pkg.is_virtual_impl() || stdlib_mi_path(root, target_backend, &pkg.fqn).is_file())
                    && (!pkg.has_implementation()
                        || stdlib_core_path(root, target_backend, &pkg.fqn).is_file())
            })
    });
    let target_layout = TargetLayout::from_resolved_project(
        target_dir.to_owned(),
        resolved_project,
        opt_level,
        action,
    );
    let artifact_paths = ArtifactPathResolver::new(target_layout, stdlib_path);
    CompileConfig {
        backend,
        opt_level,
        action,
        debug_info,
        artifact_paths,
        enable_coverage: false,
        debug_export_build_plan: false,
        moonc_output_json: false,
        docs_serve: false,
        warning_condition: WarningCondition::Default,
        warn_list: None,
    }
}
