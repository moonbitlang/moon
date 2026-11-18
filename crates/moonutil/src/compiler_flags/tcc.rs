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

use super::{ARKind, CC};

pub(super) fn default_archiver(cc_path: &Path) -> (ARKind, String) {
    (ARKind::TccAr, cc_path.display().to_string())
}

pub(super) fn resolve_archiver_path(cc: &mut CC) {
    cc.ar_path.clone_from(&cc.cc_path);
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
pub(super) fn macos_sdk_library_path() -> Option<&'static Path> {
    MACOS_SDK_LIB_PATH.as_deref()
}

#[cfg(not(target_os = "macos"))]
pub(super) fn macos_sdk_library_path() -> Option<&'static Path> {
    None
}
