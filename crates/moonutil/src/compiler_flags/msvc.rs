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

use std::path::Path;

#[cfg(windows)]
use super::ToolchainSource;
#[cfg(windows)]
use anyhow::Context;

use super::{
    CC, CCConfig, CCKind, LinkerConfig, MsvcEnvironment, OutputType, Toolchain,
    resolve_native_toolchain_executables,
};

pub const WINDOWS_MSVC_DEFAULT_LIBS: &[&str] = &[
    "libcmt.lib",
    "oldnames.lib",
    "kernel32.lib",
    "shell32.lib",
    "user32.lib",
    "dbghelp.lib",
    "uuid.lib",
];
pub const WINDOWS_MSVC_STATIC_RUNTIME_FLAG: &str = "/MT";
pub const WINDOWS_MSVC_C_STANDARD_FLAG: &str = "/std:c11";

#[cfg(windows)]
static WINDOWS_CL_TOOLCHAIN: std::sync::OnceLock<Option<DiscoveredMsvcToolchain>> =
    std::sync::OnceLock::new();
#[cfg(windows)]
static WINDOWS_CLANG_CL_TOOLCHAIN: std::sync::OnceLock<Option<DiscoveredMsvcToolchain>> =
    std::sync::OnceLock::new();

#[derive(Clone, Debug)]
struct DiscoveredMsvcToolchain {
    cc: CC,
    environment: MsvcEnvironment,
}

#[cfg(windows)]
pub(super) fn windows_msvc_host_target_triple() -> Option<String> {
    let arch = std::env::consts::ARCH;
    match arch {
        "x86_64" | "aarch64" => Some(format!("{arch}-pc-windows-msvc")),
        _ => None,
    }
}

#[cfg(windows)]
fn find_windows_msvc_toolchain(target: &str, compiler: &str) -> Option<DiscoveredMsvcToolchain> {
    let tool = find_msvc_tools::find_tool(target, compiler)?;
    let cc = CC::try_from_path(&tool.path().display().to_string()).ok()?;
    if !Path::new(&cc.ar_path).is_file() {
        return None;
    }
    Some(DiscoveredMsvcToolchain {
        cc,
        environment: MsvcEnvironment {
            command_env: tool
                .env()
                .into_iter()
                .map(|(key, value)| {
                    (
                        key.to_string_lossy().into_owned(),
                        value.to_string_lossy().into_owned(),
                    )
                })
                .collect(),
        },
    })
}

fn resolve_windows_msvc_discovery(kind: CCKind) -> anyhow::Result<DiscoveredMsvcToolchain> {
    #[cfg(not(windows))]
    {
        let _ = kind;
        anyhow::bail!("Windows MSVC environment resolution is only supported on Windows")
    }

    #[cfg(windows)]
    {
        let target = windows_msvc_host_target_triple()
            .context("Windows MSVC discovery currently supports 64-bit x64 and ARM64 hosts")?;

        let (cache, compiler) = match kind {
            CCKind::Msvc => (&WINDOWS_CL_TOOLCHAIN, "cl.exe"),
            CCKind::ClangCl => (&WINDOWS_CLANG_CL_TOOLCHAIN, "clang-cl.exe"),
            _ => anyhow::bail!("MSVC discovery requires a cl-compatible compiler"),
        };
        cache
            .get_or_init(|| find_windows_msvc_toolchain(&target, compiler))
            .clone()
            .with_context(|| {
                "Windows native backend requires MSVC Build Tools with C++ tools and Windows SDK"
            })
    }
}

pub(super) fn discovered_windows_msvc_toolchain() -> anyhow::Result<Toolchain> {
    let discovered = resolve_windows_msvc_discovery(CCKind::Msvc)
        .or_else(|_| resolve_windows_msvc_discovery(CCKind::ClangCl))?;
    Ok(Toolchain::from_path_probe(discovered.cc).with_msvc_environment(discovered.environment))
}

pub fn resolve_windows_msvc_toolchain() -> anyhow::Result<Toolchain> {
    resolve_native_toolchain_executables(discovered_windows_msvc_toolchain()?)
}

/// Resolve a selected bare MSVC-style request. Compiler selection is already
/// complete; discovery supplies the requested cl or the SDK for another driver.
#[cfg(windows)]
pub(super) fn resolve_requested_msvc_toolchain(
    mut cc: CC,
    source: ToolchainSource,
) -> anyhow::Result<Toolchain> {
    let discovered =
        if cc.cc_path.eq_ignore_ascii_case("cl") || cc.cc_path.eq_ignore_ascii_case("cl.exe") {
            let discovered = resolve_windows_msvc_discovery(CCKind::Msvc)?;
            cc.cc_path.clone_from(&discovered.cc.cc_path);
            discovered
        } else {
            // Resolve the requested driver before looking for SDK support, so a
            // missing clang-cl never silently selects a different compiler.
            cc.cc_path = crate::toolchain::resolve_executable(&cc.cc_path)
                .with_context(|| {
                    format!(
                        "failed to resolve native compiler executable `{}`",
                        cc.cc_path
                    )
                })?
                .display()
                .to_string();
            resolve_windows_msvc_discovery(CCKind::Msvc)
                .or_else(|_| resolve_windows_msvc_discovery(CCKind::ClangCl))?
        };
    cc.ar_path = discovered.cc.ar_path;
    Ok(Toolchain {
        cc,
        source,
        msvc_environment: Some(discovered.environment),
        cc_wrapper: None,
    })
}

pub(super) fn default_librarian(cc_path: &Path, cc_kind: CCKind) -> String {
    let lib = CC::resolve_tool_path(cc_path, "lib.exe");
    if Path::new(&lib).is_file() {
        return lib;
    }

    if matches!(cc_kind, CCKind::ClangCl) {
        let llvm_lib = CC::resolve_tool_path(cc_path, "llvm-lib.exe");
        if Path::new(&llvm_lib).is_file() {
            return llvm_lib;
        }
    }

    lib
}

pub(super) fn add_linker_specific_flags(cc: &CC, buf: &mut Vec<String>) {
    if cc.uses_msvc_cli() {
        buf.push("/nologo".to_string());
    }
}

pub(super) fn add_linker_runtime<P: AsRef<Path>>(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &LinkerConfig<P>,
    lpath: &str,
) {
    if cc.uses_msvc_cli() {
        if let Some(dyn_lib_path) = config.link_shared_runtime.as_ref() {
            buf.push(
                dunce::simplified(&dyn_lib_path.as_ref().join("libruntime.lib"))
                    .display()
                    .to_string(),
            );
        }
        buf.push("/link".to_string());
        buf.push(format!("/LIBPATH:{lpath}"));
    }
}

pub(super) fn add_cc_specific_flags(cc: &CC, buf: &mut Vec<String>, has_user_flags: bool) {
    if !cc.uses_msvc_cli() {
        return;
    }

    buf.push(WINDOWS_MSVC_C_STANDARD_FLAG.to_string());
    // Large generated C files can exceed the default COFF section limit.
    buf.push("/bigobj".to_string());

    if matches!(cc.cc_kind, CCKind::ClangCl) {
        // Generated C relies on wrapping signed arithmetic. Older clang-cl
        // versions otherwise optimize as if signed overflow cannot occur.
        buf.push("/clang:-fwrapv".to_string());
    }

    if !has_user_flags {
        buf.push("/utf-8".to_string());
        buf.push("/wd4819".to_string());
    }
    buf.push("/nologo".to_string());
}

pub(super) fn add_cc_runtime_flags(cc: &CC, toolchain: Option<&Toolchain>, buf: &mut Vec<String>) {
    if cc.uses_msvc_cli()
        && let Some(crt) = toolchain.and_then(Toolchain::msvc_crt_policy)
    {
        buf.push(crt.compiler_flag().to_string());
    }
}

pub(super) fn add_cc_linker_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig, lpath: &str) {
    if cc.uses_msvc_cli() && config.output_ty != OutputType::Object {
        buf.push("/link".to_string());
        buf.push(format!("/LIBPATH:{lpath}"));
    }
}
