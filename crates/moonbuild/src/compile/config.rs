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

use moonbuild_rupes_recta::BuildConfiguration;
use moonbuild_rupes_recta::{
    CompileConfig, NativeCodegenRequest, ResolvedProject, RunMode, TargetConfiguration,
    model::{
        BackendConfig, DebugInfoRequest, ENV_MOONBIT_NEW_NATIVE, NativeTarget, OperatingSystem,
    },
    target_layout::{ArtifactPathResolver, TargetLayout, stdlib_core_path, stdlib_mi_path},
};
use moonutil::{compiler_flags, resolution::ModuleSourceKind};
use std::path::Path;

/// Capture only the environment inputs used by the selected backend.
pub fn prepare_backend(
    target: TargetConfiguration,
    output_wat: bool,
    wasi_link: bool,
) -> anyhow::Result<BackendConfig> {
    let collect_ref_cycle = std::env::var("MOON_COLLECT_REF_CYCLE").as_deref() == Ok("1");
    Ok(match target {
        TargetConfiguration::Wasm => BackendConfig::Wasm {
            use_wat: output_wat,
            wasi_link: wasi_link && std::env::var("MOON_WASI_LINK").as_deref() != Ok("0"),
            new_allocator: std::env::var("MOON_WASM_NEW_ALLOCATOR").as_deref() == Ok("1"),
            collect_ref_cycle,
            separate_compilation: std::env::var("MOON_SEPARATE_COMPILATION").as_deref() == Ok("1"),
        },
        TargetConfiguration::WasmGc => BackendConfig::WasmGc {
            use_wat: output_wat,
        },
        TargetConfiguration::Js => BackendConfig::Js,
        TargetConfiguration::Native { codegen } => {
            let direct_object_candidate = if codegen == NativeCodegenRequest::Auto {
                NativeTarget::from_host_with_new_native_env(
                    std::env::consts::ARCH,
                    std::env::consts::OS,
                    std::env::var(ENV_MOONBIT_NEW_NATIVE).ok().as_deref(),
                )
            } else {
                None
            };
            BackendConfig::Native {
                direct_object_candidate,
                allocator: compiler_flags::NativeAllocator::from_env()?,
                collect_ref_cycle: codegen == NativeCodegenRequest::Auto && collect_ref_cycle,
                os: std::env::consts::OS
                    .parse::<OperatingSystem>()
                    .expect("Unknown"),
                compiler_paths: compiler_flags::CompilerPaths::from_moon_dirs(),
            }
        }
    })
}

/// Prepare artifact paths and compiler settings for the selected configuration.
/// The prepared backend must match that selection; `CompileConfig` validates it.
// TODO: Once backend generation options such as WAT belong to the semantic
// configuration, prepare these resources here instead of accepting a separately
// constructed BackendConfig from the caller.
pub fn prepare_config(
    resolved_project: &ResolvedProject,
    target_dir: &Path,
    configuration: BuildConfiguration,
    backend: BackendConfig,
    action: RunMode,
    debug_info: DebugInfoRequest,
) -> CompileConfig {
    let target_backend = configuration.target_backend();
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
            .flat_map(|(_, pkg)| {
                // A virtual implementation uses its contract's interface.
                let interface = (!pkg.is_virtual_impl())
                    .then(|| stdlib_mi_path(root, target_backend, &pkg.fqn));
                let core_ir = pkg
                    .has_implementation()
                    .then(|| stdlib_core_path(root, target_backend, &pkg.fqn));
                interface.into_iter().chain(core_ir)
            })
            .all(|artifact| artifact.is_file())
    });
    let target_layout = TargetLayout::from_resolved_project(
        target_dir.to_owned(),
        resolved_project,
        configuration,
        action,
    );
    let mut config = CompileConfig::new(
        configuration,
        backend,
        action,
        ArtifactPathResolver::new(target_layout, stdlib_path),
    );
    config.debug_info = debug_info;
    config
}
