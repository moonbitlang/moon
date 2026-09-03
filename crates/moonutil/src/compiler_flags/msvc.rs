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

use std::path::Path;

#[cfg(windows)]
use anyhow::Context;

use super::{
    CC, CCConfig, ENV_CC, LinkerConfig, MsvcEnvironment, OutputType, Toolchain, ToolchainSource,
    is_path_like_tool, resolve_native_toolchain_executables,
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
static WINDOWS_MSVC_TOOLCHAIN: std::sync::OnceLock<Option<DiscoveredMsvcToolchain>> =
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
fn find_windows_msvc_toolchain(target: &str) -> Option<DiscoveredMsvcToolchain> {
    let tool = find_msvc_tools::find_tool(target, "cl.exe")
        .or_else(|| find_msvc_tools::find_tool(target, "clang-cl.exe"))?;
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

fn resolve_windows_msvc_discovery() -> anyhow::Result<DiscoveredMsvcToolchain> {
    #[cfg(not(windows))]
    {
        anyhow::bail!("Windows MSVC environment resolution is only supported on Windows")
    }

    #[cfg(windows)]
    {
        let target = windows_msvc_host_target_triple()
            .context("Windows MSVC discovery currently supports 64-bit x64 and ARM64 hosts")?;

        WINDOWS_MSVC_TOOLCHAIN
            .get_or_init(|| find_windows_msvc_toolchain(&target))
            .clone()
            .with_context(|| {
                "Windows native backend requires MSVC Build Tools with C++ tools and Windows SDK"
            })
    }
}

pub(super) fn discovered_windows_msvc_toolchain() -> anyhow::Result<Toolchain> {
    let discovered = resolve_windows_msvc_discovery()?;
    Ok(Toolchain::from_path_probe(discovered.cc).with_msvc_environment(discovered.environment))
}

pub fn resolve_windows_msvc_toolchain() -> anyhow::Result<Toolchain> {
    resolve_native_toolchain_executables(discovered_windows_msvc_toolchain()?)
}

pub(super) fn ensure_windows_msvc_compatible(cc: &CC) -> anyhow::Result<()> {
    if cc.is_msvc() {
        Ok(())
    } else {
        anyhow::bail!(
            "Windows native backend requires an MSVC cl-compatible compiler driver such as cl.exe or clang-cl.exe; found {}",
            cc.cc_path
        )
    }
}

pub fn windows_msvc_native_toolchain(package_cc: Option<&CC>) -> anyhow::Result<Toolchain> {
    if let Some(env_cc) = ENV_CC.as_ref().filter(|cc| cc.is_msvc()) {
        let override_toolchain = Toolchain::from_env_override(env_cc.clone());
        let resolved = if is_path_like_tool(&env_cc.cc_path) {
            override_toolchain
        } else {
            let discovered = discovered_windows_msvc_toolchain()?;
            resolve_msvc_toolchain_override(override_toolchain, &discovered)
        };
        return windows_msvc_toolchain_with_package_override(resolved, package_cc);
    }

    if let Some(package_cc) = package_cc {
        ensure_windows_msvc_compatible(package_cc)?;
    }

    let resolved = discovered_windows_msvc_toolchain()?;
    windows_msvc_toolchain_with_package_override(resolved, package_cc)
}

pub fn has_incompatible_windows_msvc_env_override() -> bool {
    ENV_CC.as_ref().is_some_and(|cc| !cc.is_msvc())
}

pub(super) fn windows_msvc_toolchain_with_package_override(
    resolved: Toolchain,
    package_cc: Option<&CC>,
) -> anyhow::Result<Toolchain> {
    let mut toolchain = resolved.with_package_override(package_cc);
    ensure_windows_msvc_compatible(toolchain.cc())?;

    if toolchain.source() == ToolchainSource::PackageOverride {
        toolchain = resolve_msvc_toolchain_override(toolchain, &resolved);
    }

    resolve_native_toolchain_executables(toolchain)
}

// A bare override selects the discovered toolchain as a whole. Path-like overrides
// remain self-contained so they cannot inherit an environment for another installation.
pub(super) fn resolve_msvc_toolchain_override(
    mut toolchain: Toolchain,
    resolved: &Toolchain,
) -> Toolchain {
    if is_path_like_tool(&toolchain.cc.cc_path) {
        return toolchain;
    }

    toolchain.cc.cc_path.clone_from(&resolved.cc.cc_path);
    toolchain.cc.ar_path.clone_from(&resolved.cc.ar_path);

    match resolved.msvc_environment() {
        Some(environment) => toolchain.with_msvc_environment(environment.clone()),
        None => toolchain,
    }
}

pub(super) fn default_librarian(cc_path: &Path) -> String {
    let lib = CC::resolve_tool_path(cc_path, "lib.exe");
    if Path::new(&lib).is_file() {
        return lib;
    }

    let compiler_name = cc_path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if CC::strip_exe_suffix(&compiler_name).ends_with("clang-cl") {
        let llvm_lib = CC::resolve_tool_path(cc_path, "llvm-lib.exe");
        if Path::new(&llvm_lib).is_file() {
            return llvm_lib;
        }
    }

    lib
}

pub(super) fn add_linker_specific_flags(cc: &CC, buf: &mut Vec<String>) {
    if cc.is_msvc() {
        buf.push("/nologo".to_string());
    }
}

pub(super) fn add_linker_runtime<P: AsRef<Path>>(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &LinkerConfig<P>,
    lpath: &str,
) {
    if cc.is_msvc() {
        if let Some(dyn_lib_path) = config.link_shared_runtime.as_ref() {
            buf.push(
                dyn_lib_path
                    .as_ref()
                    .join("libruntime.lib")
                    .display()
                    .to_string(),
            );
        }
        buf.push("/link".to_string());
        buf.push(format!("/LIBPATH:{lpath}"));
    }
}

pub(super) fn add_cc_specific_flags(cc: &CC, buf: &mut Vec<String>, has_user_flags: bool) {
    if !cc.is_msvc() {
        return;
    }

    buf.push(WINDOWS_MSVC_C_STANDARD_FLAG.to_string());

    if !has_user_flags {
        buf.push("/utf-8".to_string());
        buf.push("/wd4819".to_string());
    }
    buf.push("/nologo".to_string());
}

pub(super) fn add_cc_runtime_flags(cc: &CC, toolchain: Option<&Toolchain>, buf: &mut Vec<String>) {
    if cc.is_msvc()
        && let Some(crt) = toolchain.and_then(Toolchain::msvc_crt_policy)
    {
        buf.push(crt.compiler_flag().to_string());
    }
}

pub(super) fn add_cc_linker_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig, lpath: &str) {
    if cc.is_msvc() && config.output_ty != OutputType::Object {
        buf.push("/link".to_string());
        buf.push(format!("/LIBPATH:{lpath}"));
    }
}
