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

use colored::Colorize;

use super::{ARKind, CC, CCConfig};

pub(super) fn default_archiver(cc_path: &Path) -> (ARKind, String) {
    (ARKind::TccAr, cc_path.display().to_string())
}

pub(super) fn resolve_archiver_path(cc: &mut CC) {
    cc.ar_path.clone_from(&cc.cc_path);
}

pub(super) fn add_archiver_flags(buf: &mut Vec<String>, dest: &str) {
    buf.push("-ar".to_string());
    buf.push("rcs".to_string());
    buf.push(dest.to_string());
}

#[cfg(target_os = "macos")]
fn resolve_macos_sdk_lib_path() -> Option<std::path::PathBuf> {
    let output = std::process::Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let sdk_root = String::from_utf8_lossy(&output.stdout);
    let sdk_root = sdk_root.lines().next()?.trim();
    if sdk_root.is_empty() {
        return None;
    }

    let sdk_lib_path = Path::new(sdk_root).join("usr").join("lib");
    sdk_lib_path.is_dir().then_some(sdk_lib_path)
}

#[cfg(target_os = "macos")]
static MACOS_SDK_LIB_PATH: std::sync::LazyLock<Option<std::path::PathBuf>> =
    std::sync::LazyLock::new(resolve_macos_sdk_lib_path);

#[cfg(target_os = "macos")]
pub(super) fn add_macos_sdk_library_path(buf: &mut Vec<String>) {
    if let Some(sdk_lib_path) = MACOS_SDK_LIB_PATH.as_ref() {
        buf.push(format!("-L{}", sdk_lib_path.display()));
    }
}

#[cfg(not(target_os = "macos"))]
pub(super) fn add_macos_sdk_library_path(_buf: &mut Vec<String>) {}

pub(super) fn add_cc_specific_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig) {
    if !cc.is_tcc() {
        return;
    }

    if config.no_sys_header {
        buf.push("-DMOONBIT_NATIVE_NO_SYS_HEADER".to_string());
    } else {
        eprintln!(
            "{}: Use tcc without set MOONBIT_NATIVE_NO_SYS_HEADER.",
            "Warning".yellow().bold(),
        );
    }
}
