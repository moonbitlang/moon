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

//! Abstraction for `moonc link-core`.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use moonutil::common::TargetBackend;
use moonutil::package::{ImportMemory, JsFormat, MemoryLimits};

use crate::build_lower::compiler::{
    CmdlineAbstraction, CompilationFlags, CompiledPackageName, PackageSource,
};

/// Abstraction for `moonc link-core`.
///
/// This struct reuses existing structures and mimics the legacy behavior
/// as much as possible, maintaining EXACT argument order.
#[derive(Debug)]
pub struct MooncLinkCore<'a> {
    // Input/Output configuration
    /// The `.core` file dependencies to link.
    ///
    /// Due to `moonc`'s restrictions, this list **must be in topological-sorted
    /// order**. Failing to do so will result in a mismatch in the initializer
    /// order of the linked output, and eventually lead to runtime errors.
    pub core_deps: &'a [PathBuf],
    /// THe main package to link. This package should contain the `main`
    /// function/entry point, if applicable.
    pub main_package: CompiledPackageName<'a>,
    /// The linked output file's path.
    ///
    /// This should be the `.wasm`/`.wat`/`.js`/`.c`/`.o`/`.obj` file depending
    /// on the target backend and platform.
    pub output_path: Cow<'a, Path>,
    /// The path to the `moon.pkg.json` file of the main package.
    pub pkg_config_path: Cow<'a, Path>,

    // Package configuration
    /// The package name to source path mapping of all packages involved in the build.
    pub package_sources: &'a [PackageSource<'a>],
    /// Pass [None] for no_std, otherwise provide PackageSource for core
    pub stdlib_core_source: Option<PackageSource<'a>>,

    // Target and compilation configuration
    /// The target backend to link for.
    pub target_backend: TargetBackend,
    /// Compilation flags - reuse existing structure, symbols maps to -g, no_opt maps to -O0
    pub flags: CompilationFlags,
    /// Whether this project is linked to be used for testing. Test projects
    /// enables special configs that is not applicable to a normal
    /// executable/library.
    pub test_mode: bool,
    /// List of functions to export in the final (WebAssembly/JS) output.
    pub exports: Option<&'a [String]>,

    /// WebAssembly specific configuration
    pub wasm_config: WasmConfig<'a>,

    /// JavaScript specific configuration
    pub js_config: Option<JsConfig>,

    /// Extra options passed from user configuration.
    pub extra_link_opts: &'a [String],
}

/// WebAssembly-specific linking configuration
#[derive(Debug, Default)]
pub struct WasmConfig<'a> {
    /// The name of the exported WASM memory, if any.
    ///
    /// See: https://www.w3.org/TR/2019/REC-wasm-core-1-20191205/#exports%E2%91%A0
    pub export_memory_name: Option<Cow<'a, str>>,
    /// The import memory configuration, if any.
    ///
    /// See: https://www.w3.org/TR/2019/REC-wasm-core-1-20191205/#imports%E2%91%A0
    pub import_memory: Option<&'a ImportMemory>,
    /// Memory limits configuration
    ///
    /// See: https://www.w3.org/TR/2019/REC-wasm-core-1-20191205/#syntax-limits
    pub memory_limits: Option<&'a MemoryLimits>,
    /// Whether to enable shared memory
    ///
    /// See: https://developer.mozilla.org/en-US/docs/WebAssembly/Guides/Understanding_the_text_format#shared_memories
    pub shared_memory: Option<bool>,
    /// The starting address of the heap in WASM linear memory.
    ///
    /// As the WASM linear memory only grows upwards, this affects the size of
    /// the stack data segment. A value too small may cause the stack to be too
    /// small and overflow during runtime, while a value too large may waste
    /// memory.
    pub heap_start_address: Option<u32>,
    /// Extra link flags to pass to the WASM linker.
    pub link_flags: Option<&'a [String]>,
}

/// JavaScript-specific linking configuration
#[derive(Debug)]
pub struct JsConfig {
    /// The output format of the script, see [JsFormat].
    pub format: Option<JsFormat>,
    /// Whether to skip generating TypeScript declaration files.
    pub no_dts: bool,
}

impl<'a> MooncLinkCore<'a> {
    /// Convert this to list of args. The behavior tries to mimic the legacy
    /// behavior as much as possible, maintaining EXACT argument order.
    pub fn to_args_legacy(&self, args: &mut Vec<String>) {
        // Command name
        args.push("link-core".into());

        // Core dependencies (input files) - first in legacy order
        for core_dep in self.core_deps {
            args.push(core_dep.to_string_lossy().into_owned());
        }

        // Main package specification
        args.push("-main".to_string());
        args.push(self.main_package.to_string());

        // Output file
        args.push("-o".to_string());
        args.push(self.output_path.display().to_string());

        if self.test_mode {
            args.push("-test-mode".to_string());
        }

        // Package configuration path
        args.push("-pkg-config-path".to_string());
        args.push(self.pkg_config_path.display().to_string());

        // Package sources (using existing to_arg method)
        for pkg_source in self.package_sources {
            args.push("-pkg-sources".to_string());
            args.push(pkg_source.to_arg());
        }

        // Standard library (if not no_std)
        if let Some(ref stdlib_core) = self.stdlib_core_source {
            args.push("-pkg-sources".to_string());
            args.push(stdlib_core.to_arg());
        }

        if self.test_mode {
            args.push("-exported_functions".to_string());
            args.push(
                "moonbit_test_driver_internal_execute,moonbit_test_driver_finish".to_string(),
            );
        }

        // JavaScript configuration
        if let Some(js_config) = &self.js_config {
            if let Some(format) = js_config.format {
                args.push("-js-format".to_string());
                args.push(format.to_flag().to_string());
            }
            if js_config.no_dts {
                args.push("-no-dts".to_string());
            }
        }

        // Target backend
        args.push("-target".to_string());
        args.push(self.target_backend.to_flag().to_string());

        // Debug and optimization flags (using CompilationFlags)
        // symbols maps to -g, no_opt maps to -O0
        if self.flags.symbols {
            args.push("-g".to_string());
        }
        if self.flags.no_opt {
            args.push("-O0".to_string());
        }

        // Source map
        if self.flags.source_map {
            args.push("-source-map".to_string());
        }

        // WebAssembly exports
        if let Some(exports) = self.exports
            && !exports.is_empty()
            && !self.test_mode
        // when it's test mode, we only export test driver utils
        {
            args.push(format!("-exported_functions={}", exports.join(",")));
        }

        // WASM-specific config
        if self.target_backend.is_wasm() {
            // Export memory name
            if let Some(export_memory_name) = &self.wasm_config.export_memory_name {
                args.push("-export-memory-name".to_string());
                args.push(export_memory_name.to_string());
            }

            // Import memory
            if let Some(import_memory) = self.wasm_config.import_memory {
                args.push("-import-memory-module".to_string());
                args.push(import_memory.module.clone());
                args.push("-import-memory-name".to_string());
                args.push(import_memory.name.clone());
            }

            // Memory limits
            if let Some(memory_limits) = self.wasm_config.memory_limits {
                args.push("-memory-limits-min".to_string());
                args.push(memory_limits.min.to_string());
                args.push("-memory-limits-max".to_string());
                args.push(memory_limits.max.to_string());
            }

            // Shared memory
            if let Some(shared_memory) = self.wasm_config.shared_memory
                && shared_memory
            {
                args.push("-shared-memory".to_string());
            }

            // Heap start address
            if let Some(heap_start_address) = self.wasm_config.heap_start_address {
                args.push("-heap-start-address".to_string());
                args.push(heap_start_address.to_string());
            }

            // Link flags
            if let Some(link_flags) = self.wasm_config.link_flags {
                for flag in link_flags {
                    args.push(flag.clone());
                }
            }
        }

        // Extra link options
        for opt in self.extra_link_opts {
            args.push(opt.to_string());
        }

        // Windows-specific LLVM target workaround (conditional compilation like legacy)
        // FIXME: We should always provide target info for LLVM
        #[cfg(target_os = "windows")]
        if self.target_backend == TargetBackend::LLVM {
            use moonutil::compiler_flags::CC;
            if CC::default().is_msvc() {
                args.push("-llvm-target".to_string());
                args.push("x86_64-pc-windows-msvc".to_string());
            }
        }
    }
}

impl CmdlineAbstraction for MooncLinkCore<'_> {
    fn to_args(&self, args: &mut Vec<String>) {
        self.to_args_legacy(args);
    }
}
