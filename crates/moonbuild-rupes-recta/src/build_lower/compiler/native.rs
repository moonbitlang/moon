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

//! Native compiler, linker, and archiver commands owned by build lowering.
//!
//! Optional SDK and library queries remain lazy in the toolchain module.
//! TODO: Supply their observations from build preparation when native external
//! inputs are captured before lowering; command construction must not grow its
//! own filesystem or process probes.

use std::path::{Path, PathBuf};

use moonutil::compiler_flags::{CC, CompilerPaths, NativeAllocator, Toolchain};

use crate::model::{CCOptLevel as OptLevel, NativeLinkArg};

use super::super::command::BuildCommand;
mod archiver;
mod gnu;
pub(in crate::build_lower) mod msvc;
mod tcc;

pub(in crate::build_lower) use archiver::{ArchiverConfig, make_archiver_command};

#[derive(Clone, Copy, PartialEq)]
pub(in crate::build_lower) enum OutputType {
    Object,     // .o or .obj
    SharedLib,  // .so or .dll or .dylib
    Executable, // .exe or no extension
}

#[derive(Clone)]
pub(in crate::build_lower) struct CCConfig {
    // indicates -g for gcc-like compilers
    // we don't set /Zi as it will have concurrency problem
    // like multiple msvc instances race to write the same .pdb file
    pub debug_info: bool,
    // TCC cannot link libmoonbitrun.o
    pub link_moonbitrun: bool,
    // Link libbacktrace.a from the configured MoonBit lib path if it exists
    // and the compiler targets the host.
    pub link_libbacktrace: bool,
    // Define MOONBIT_NATIVE_NO_SYS_HEADER
    // Usually used with TCC
    // TCC may not be able to handle the system header
    pub no_sys_header: bool,
    pub output_ty: OutputType,
    pub opt_level: OptLevel,
    // Define MOONBIT_ALLOW_STACKTRACE
    pub allow_stacktrace: bool,
    // Define MOONBIT_USE_SHARED_RUNTIME
    // It's non-op on Linux and MacOS
    // But on Windows, it will mark runtime function declarations
    // with extra __declspec(dllimport)
    // This is needed to use the shared runtime
    pub define_use_shared_runtime_macro: bool,
    // Define MOONBIT_USE_SIMDUTF.
    pub use_simdutf: bool,
    // Define MOONBIT_TRIAL_DELETION=1 for native runtime and program compilation.
    pub collect_ref_cycle: bool,
    // Select native runtime allocator when this command participates in
    // runtime compilation or native executable linking.
    pub native_allocator: Option<NativeAllocator>,
}

#[derive(Clone)]
pub(in crate::build_lower) struct LinkerConfig<P: AsRef<Path>> {
    /// Compiler-driver options that also affect linking, before MSVC's `/link`.
    pub driver_flags: Vec<String>,
    pub debug_info: bool,
    /// Retain the generated-C/LLVM driver's optimization policy for LTO.
    pub opt_level: Option<OptLevel>,
    pub link_moonbitrun: bool,
    // Link libbacktrace.a from the configured MoonBit lib path if it exists
    // and the compiler targets the host.
    pub link_libbacktrace: bool,
    pub output_ty: OutputType,
    // This is the parent directory to the shared runtime library
    pub link_shared_runtime: Option<P>,
    pub native_allocator: NativeAllocator,
}

impl<P: AsRef<Path>> Default for LinkerConfig<P> {
    fn default() -> Self {
        Self {
            driver_flags: Vec::new(),
            debug_info: false,
            opt_level: None,
            link_moonbitrun: false,
            link_libbacktrace: false,
            output_ty: OutputType::Executable,
            link_shared_runtime: None,
            native_allocator: NativeAllocator::default(),
        }
    }
}

/// Render structured link inputs only after the compiler driver is selected.
/// MSVC callers place these after `/link`; plain clang uses GCC-style arguments
/// even when its target ABI is MSVC.
fn render_link_arg(arg: &NativeLinkArg, cc: &CC) -> String {
    match arg {
        NativeLinkArg::Library(name) if cc.uses_msvc_cli() => format!("{name}.lib"),
        NativeLinkArg::Library(name) => format!("-l{name}"),
        NativeLinkArg::SearchPath(path) if cc.uses_msvc_cli() => {
            format!("/LIBPATH:{}", path.display())
        }
        NativeLinkArg::SearchPath(path) => format!("-L{}", path.display()),
        NativeLinkArg::Opaque(arg) => arg.clone(),
    }
}

// Helper functions for linker command building
fn add_linker_output_flags(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &LinkerConfig<impl AsRef<Path>>,
    dest: &str,
) {
    if cc.uses_msvc_cli() {
        match config.output_ty {
            OutputType::SharedLib | OutputType::Executable => {
                // /F(executable)
                buf.push(format!(
                    "/Fe{}",
                    dunce::simplified(Path::new(dest)).display()
                ));
            }
            _ => panic!("Linker only supports shared lib, executable and static lib"),
        }
    } else if cc.is_gcc_like() {
        buf.push("-o".to_string());
        buf.push(dunce::simplified(Path::new(dest)).display().to_string());
    }
}

fn add_linker_library_paths<P: AsRef<Path>>(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &LinkerConfig<P>,
    lpath: &str,
) {
    if cc.is_tcc() {
        if let Some(path) = cc.macos_sdk_library_path() {
            buf.push(format!("-L{}", path.display()));
        }
        buf.push(format!(
            "-L{}",
            dunce::simplified(Path::new(lpath)).display()
        ));
    }
    if cc.is_gcc_like()
        && let Some(dyn_lib_path) = config.link_shared_runtime.as_ref()
    {
        buf.push(format!("-L{}", dyn_lib_path.as_ref().display()));
    }
}

fn add_linker_intermediate_dir_flags(cc: &CC, buf: &mut Vec<String>, dest_dir: &str) {
    // MSVC may throw intermediate files into current directory
    // Explicitly set the output directory of these files
    if cc.uses_msvc_cli() {
        // /F(object)
        buf.push(format!("/Fo{dest_dir}\\"));
    }
}

fn add_linker_shared_lib_flags(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &LinkerConfig<impl AsRef<Path>>,
) {
    if config.output_ty == OutputType::SharedLib {
        if cc.uses_msvc_cli() {
            buf.push("/LD".to_string());
        } else if cc.is_gcc_like() {
            buf.push("-shared".to_string());
            buf.push("-fPIC".to_string());
        }
    }
}

// Keep selected toolchain libraries alongside their arguments. Lowering must not
// recover file inputs by parsing the rendered command line.
fn add_library_input(buf: &mut Vec<String>, inputs: &mut Vec<PathBuf>, path: PathBuf) {
    buf.push(dunce::simplified(&path).display().to_string());
    inputs.push(path);
}

pub(in crate::build_lower) fn make_linker_command<S, P>(
    cc: CC,
    config: LinkerConfig<P>,
    link_args: &[NativeLinkArg],
    src: &[S],
    dest_dir: &str,
    dest: &str,
    paths: &CompilerPaths,
) -> BuildCommand
where
    S: AsRef<str>,
    P: AsRef<Path>,
{
    let mut buf = vec![cc.cc_path.clone()];
    let mut inputs = Vec::new();
    let lpath = &paths.lib_path;
    add_linker_output_flags(&cc, &mut buf, &config, dest);
    add_linker_library_paths(&cc, &mut buf, &config, lpath);
    add_linker_intermediate_dir_flags(&cc, &mut buf, dest_dir);
    add_linker_shared_lib_flags(&cc, &mut buf, &config);
    add_cc_debug_flags(&cc, &mut buf, config.debug_info);
    if let Some(opt_level) = config.opt_level {
        add_cc_optimization_flags(&cc, &mut buf, opt_level, !config.driver_flags.is_empty());
    }

    // Linker compiler-specific flags
    msvc::add_linker_specific_flags(&cc, &mut buf);

    if config.link_moonbitrun
        && let Some(path) = paths.moonbitrun_object(&cc, config.native_allocator)
    {
        add_library_input(&mut buf, &mut inputs, path);
    }

    buf.extend(src.iter().map(|s| {
        dunce::simplified(Path::new(s.as_ref()))
            .display()
            .to_string()
    }));

    gnu::add_linker_common_libraries(&cc, &mut buf, &config);
    buf.extend(config.driver_flags.iter().cloned());
    msvc::add_linker_runtime(&cc, &mut buf, &config, lpath);

    buf.extend(link_args.iter().map(|arg| render_link_arg(arg, &cc)));
    if config.link_libbacktrace
        && let Some(path) = paths.libbacktrace_path(&cc)
    {
        add_library_input(&mut buf, &mut inputs, path);
    }

    BuildCommand {
        commandline: buf.into(),
        extra_inputs: inputs,
    }
}

// Helper functions for CC command building
fn add_cc_output_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig, dest: Option<&str>) {
    let Some(dest) = dest else {
        return;
    };
    if cc.uses_msvc_cli() {
        match config.output_ty {
            OutputType::Object => {
                buf.push(format!(
                    "/Fo{}",
                    dunce::simplified(Path::new(dest)).display()
                ));
            }
            OutputType::SharedLib | OutputType::Executable => {
                buf.push(format!(
                    "/Fe{}",
                    dunce::simplified(Path::new(dest)).display()
                ));
            }
        }
    } else if cc.is_gcc_like() {
        buf.push("-o".to_string());
        buf.push(dunce::simplified(Path::new(dest)).display().to_string());
    }
}

fn add_cc_include_and_lib_paths(cc: &CC, buf: &mut Vec<String>, ipath: &str, lpath: &str) {
    if cc.uses_msvc_cli() {
        buf.push(format!(
            "/I{}",
            dunce::simplified(Path::new(ipath)).display()
        ));
    } else if cc.is_tcc() {
        buf.push(format!(
            "-I{}",
            dunce::simplified(Path::new(ipath)).display()
        ));
        if let Some(path) = cc.macos_sdk_library_path() {
            buf.push(format!("-L{}", path.display()));
        }
        buf.push(format!(
            "-L{}",
            dunce::simplified(Path::new(lpath)).display()
        ));
    } else if cc.is_gcc_like() {
        buf.push(format!(
            "-I{}",
            dunce::simplified(Path::new(ipath)).display()
        ));
    }
}

fn add_cc_intermediate_dir_flags(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &CCConfig,
    dest_dir: &str,
) {
    // MSVC may throw intermediate files into current directory
    // Explicitly set the output directory of these files
    if cc.uses_msvc_cli() && config.output_ty != OutputType::Object {
        buf.push(format!("/Fo{dest_dir}\\"));
    }
}

fn add_cc_debug_flags(cc: &CC, buf: &mut Vec<String>, debug_info: bool) {
    if debug_info {
        if cc.is_gcc_like() {
            buf.push("-g".to_string());
        } else if cc.uses_msvc_cli() {
            buf.push("/Z7".to_string());
        }
    }
}

fn add_cc_shared_lib_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig) {
    if config.output_ty == OutputType::SharedLib {
        if cc.uses_msvc_cli() {
            buf.push("/LD".to_string());
        } else if cc.is_gcc_like() {
            buf.push("-shared".to_string());
            buf.push("-fPIC".to_string());
        }
    }
}

fn add_cc_compile_only_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig) {
    if config.output_ty == OutputType::Object {
        if cc.uses_msvc_cli() {
            buf.push("/c".to_string());
        } else if cc.is_gcc_like() {
            buf.push("-c".to_string());
        }
    }
}

fn add_cc_optimization_flags(
    cc: &CC,
    buf: &mut Vec<String>,
    opt_level: OptLevel,
    has_user_flags: bool,
) {
    if !has_user_flags {
        match opt_level {
            OptLevel::Speed => {
                if cc.uses_msvc_cli() {
                    buf.push("/O2".to_string());
                } else if cc.is_full_featured_gcc_like() {
                    buf.push("-O2".to_string());
                }
            }
            OptLevel::Debug => {
                if cc.uses_msvc_cli() {
                    buf.push("/Od".to_string());
                } else if cc.is_full_featured_gcc_like() {
                    buf.push("-Og".to_string());
                }
            }
            OptLevel::None => {
                if cc.uses_msvc_cli() {
                    buf.push("/Od".to_string());
                } else if cc.is_full_featured_gcc_like() {
                    buf.push("-O0".to_string());
                }
            }
        }
    }
}

fn add_cc_build_system_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig) {
    let prefix = if cc.uses_msvc_cli() {
        "/D"
    } else if cc.is_gcc_like() {
        "-D"
    } else {
        return;
    };
    if config.allow_stacktrace {
        buf.push(format!("{prefix}MOONBIT_ALLOW_STACKTRACE"));
    }
    if config.collect_ref_cycle {
        buf.push(format!("{prefix}MOONBIT_TRIAL_DELETION=1"));
    }
}

fn add_cc_shared_runtime_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig) {
    // always set this even if user cc flags are set
    // user cannot easily know when we use shared runtime
    if config.define_use_shared_runtime_macro {
        if cc.uses_msvc_cli() {
            buf.push("/DMOONBIT_USE_SHARED_RUNTIME".to_string());
        } else if cc.is_gcc_like() {
            buf.push("-fPIC".to_string());
            buf.push("-DMOONBIT_USE_SHARED_RUNTIME".to_string());
        }
    }
}

fn add_cc_simdutf_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig) {
    if !config.use_simdutf {
        return;
    }

    if cc.uses_msvc_cli() {
        buf.push("/DMOONBIT_USE_SIMDUTF".to_string());
    } else if cc.is_gcc_like() {
        buf.push("-DMOONBIT_USE_SIMDUTF".to_string());
    }
}

fn add_cc_allocator_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig) {
    let Some(native_allocator) = config.native_allocator else {
        return;
    };

    let allocator_macro = native_allocator.moonbit_allocator_macro(cc, config.link_moonbitrun);

    if cc.uses_msvc_cli() {
        buf.push(format!("/DMOONBIT_ALLOCATOR={allocator_macro}"));
    } else if cc.is_gcc_like() {
        buf.push(format!("-DMOONBIT_ALLOCATOR={allocator_macro}"));
    }
}

/// Construct one C compiler-driver command from the selected toolchain.
/// Link-only arguments do not suppress default compiler optimization flags.
#[allow(clippy::too_many_arguments)]
pub(in crate::build_lower) fn make_cc_command<S>(
    toolchain: &Toolchain,
    config: CCConfig,
    cc_flags: &[S],
    link_args: &[NativeLinkArg],
    src: impl IntoIterator<Item = impl Into<String>>,
    intermediate_dir: &str,
    dest: Option<&str>,
    paths: &CompilerPaths,
) -> BuildCommand
where
    S: AsRef<str>,
{
    let cc = toolchain.cc();
    let mut buf = vec![toolchain.cc_command_path()];
    let mut inputs = Vec::new();

    // If user C flags are set, we only set necessary flags
    // that are tightly coupled with the paths and output types
    // as user cannot easily specify them in the configuration file
    // Link-only flags should not affect compiler defaults.
    let has_user_flags = !cc_flags.is_empty();

    add_cc_output_flags(cc, &mut buf, &config, dest);
    add_cc_include_and_lib_paths(cc, &mut buf, &paths.include_path, &paths.lib_path);
    add_cc_intermediate_dir_flags(cc, &mut buf, &config, intermediate_dir);
    add_cc_debug_flags(cc, &mut buf, config.debug_info);
    add_cc_shared_lib_flags(cc, &mut buf, &config);
    add_cc_compile_only_flags(cc, &mut buf, &config);

    // Compiler-specific flags
    msvc::add_cc_specific_flags(cc, &mut buf, has_user_flags);
    gnu::add_cc_specific_flags(cc, &mut buf);
    tcc::add_cc_specific_flags(cc, &mut buf, &config);

    add_cc_optimization_flags(cc, &mut buf, config.opt_level, has_user_flags);
    add_cc_build_system_flags(cc, &mut buf, &config);
    add_cc_shared_runtime_flags(cc, &mut buf, &config);
    add_cc_simdutf_flags(cc, &mut buf, &config);
    add_cc_allocator_flags(cc, &mut buf, &config);
    if config.output_ty != OutputType::Object
        && config.link_moonbitrun
        && let Some(path) = paths.moonbitrun_object(cc, config.native_allocator.unwrap_or_default())
    {
        add_library_input(&mut buf, &mut inputs, path);
    }

    // MSVC rejects verbatim source arguments even when the files are short.
    // Simplify only these complete file arguments; keep output directories
    // verbatim because the compiler appends intermediate filenames.
    buf.extend(src.into_iter().map(|s| {
        let path = s.into();
        dunce::simplified(Path::new(&path)).display().to_string()
    }));

    gnu::add_cc_common_libraries(cc, &mut buf, &config);
    buf.extend(cc_flags.iter().map(|s| s.as_ref().to_string()));
    buf.extend(toolchain.msvc_crt_compiler_flag().map(str::to_owned));
    // Finish compiler options before /link; MSVC otherwise ignores linker
    // options such as prebuild-provided /LIBPATH entries.
    msvc::add_cc_linker_flags(cc, &mut buf, &config, &paths.lib_path);
    buf.extend(link_args.iter().map(|arg| render_link_arg(arg, cc)));
    if config.link_libbacktrace
        && config.output_ty != OutputType::Object
        && let Some(path) = paths.libbacktrace_path(cc)
    {
        add_library_input(&mut buf, &mut inputs, path);
    }
    BuildCommand::c_compile(toolchain, buf, inputs)
}

#[cfg(test)]
mod tests;
