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
    CompileConfig, ResolvedProject,
    build_lower::WarningCondition,
    model::{
        BackendConfig, DebugInfoRequest, ENV_MOONBIT_NEW_NATIVE, NativeTarget, OperatingSystem,
    },
    target_layout::{ArtifactPathResolver, TargetLayout},
};
use moonutil::{
    build_options::RunMode, compiler_flags, cond_expr::OptLevel as BuildProfile,
    constants::MOONBITLANG_CORE, target::TargetBackend,
};
use std::path::Path;
use tracing::info;

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
    dry_run: bool,
) -> anyhow::Result<CompileConfig> {
    let target_backend = backend.target_backend();
    let is_core = match resolved_project.local_modules() {
        &[module_id] => resolved_project.module_info(module_id).name == MOONBITLANG_CORE,
        _ => false,
    };
    info!("is_core: {}", is_core);
    let stdlib_path = (!is_core).then(moonutil::toolchain::core);
    if target_backend == TargetBackend::LLVM
        && !dry_run
        && let Some(core_root) = &stdlib_path
    {
        // Not every toolchain ships the LLVM bundle. Catch its missing prelude
        // before moonc tries to load the interface and reports an internal error.
        let prelude = moonutil::toolchain::core_package_mi_in(
            core_root,
            target_backend,
            "prelude",
            "prelude",
        );
        anyhow::ensure!(
            prelude.is_file(),
            "toolchain is incomplete: missing `prelude.mi` for target `llvm`"
        );
    }
    let target_layout = TargetLayout::from_resolved_project(
        target_dir.to_owned(),
        resolved_project,
        opt_level,
        action,
    );
    let artifact_paths = ArtifactPathResolver::new(target_layout, stdlib_path.clone());
    Ok(CompileConfig {
        target_dir: target_dir.to_owned(),
        backend,
        opt_level,
        action,
        debug_info,
        stdlib_path,
        artifact_paths,
        enable_coverage: false,
        debug_export_build_plan: false,
        moonc_output_json: false,
        docs_serve: false,
        warning_condition: WarningCondition::Default,
        warn_list: None,
    })
}
