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

use super::{ARKind, CC, CCConfig, CCKind, LinkerConfig, OutputType};

impl CC {
    fn probe_prog_name(cc_path: &Path, name: &str) -> Option<String> {
        let output = std::process::Command::new(cc_path)
            .arg(format!("-print-prog-name={name}"))
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let prog = String::from_utf8_lossy(&output.stdout);
        let prog = prog.lines().next()?.trim();
        (!prog.is_empty()).then(|| prog.to_string())
    }

    pub(super) fn resolve_reported_prog_path(prog: &str) -> Option<String> {
        let prog_path = Path::new(prog);
        let has_non_empty_parent = prog_path
            .parent()
            .is_some_and(|parent| !parent.as_os_str().is_empty());

        if prog_path.is_absolute() || has_non_empty_parent {
            if prog_path.is_file() {
                return Some(prog.to_string());
            }

            #[cfg(windows)]
            if prog_path.extension().is_none() {
                let exe_path = prog_path.with_extension("exe");
                if exe_path.is_file() {
                    return Some(exe_path.display().to_string());
                }
            }

            return None;
        }

        which::which(prog)
            .ok()
            .map(|path| path.display().to_string())
    }

    pub(super) fn probe_existing_prog_name(cc_path: &Path, name: &str) -> Option<String> {
        let prog = CC::probe_prog_name(cc_path, name)?;
        CC::resolve_reported_prog_path(&prog)
    }

    pub(super) fn with_default_platform_archiver(mut self) -> Self {
        #[cfg(target_os = "macos")]
        if self.targets_apple_darwin()
            && !self.is_tcc()
            && let Some(libtool) = resolve_apple_libtool_path()
        {
            self.ar_kind = ARKind::AppleLibtool;
            self.ar_path = libtool.display().to_string();
            return self;
        }

        if matches!(self.cc_kind, CCKind::Clang)
            && self.targets_msvc()
            && let Some(llvm_lib) =
                CC::probe_existing_prog_name(Path::new(&self.cc_path), "llvm-lib")
        {
            self.ar_kind = ARKind::MsvcLib;
            self.ar_path = llvm_lib;
        }

        self
    }

    fn is_llvm_ar_name(ar_name_or_path: &str) -> bool {
        let file_name = Path::new(ar_name_or_path)
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or(ar_name_or_path)
            .to_ascii_lowercase();
        CC::strip_exe_suffix(&file_name) == "llvm-ar"
    }

    fn is_msvc_librarian_name(ar_name_or_path: &str) -> bool {
        let file_name = Path::new(ar_name_or_path)
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or(ar_name_or_path)
            .to_ascii_lowercase();
        matches!(CC::strip_exe_suffix(&file_name), "lib" | "llvm-lib")
    }

    fn is_apple_libtool_name(ar_name_or_path: &str) -> bool {
        let file_name = Path::new(ar_name_or_path)
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or(ar_name_or_path)
            .to_ascii_lowercase();
        CC::strip_exe_suffix(&file_name) == "libtool"
    }

    pub(super) fn classify_gcc_like_archiver(
        ar_name_or_path: &str,
        target_triple: Option<&str>,
    ) -> ARKind {
        if target_triple.is_some_and(|target| target.contains("msvc"))
            && CC::is_msvc_librarian_name(ar_name_or_path)
        {
            ARKind::MsvcLib
        } else if target_triple.is_some_and(|target| target.contains("apple-darwin"))
            && CC::is_apple_libtool_name(ar_name_or_path)
        {
            ARKind::AppleLibtool
        } else if CC::is_llvm_ar_name(ar_name_or_path) {
            ARKind::LlvmAr
        } else {
            ARKind::GnuAr
        }
    }
}

#[cfg(target_os = "macos")]
pub(super) fn resolve_apple_libtool_path() -> Option<std::path::PathBuf> {
    let output = std::process::Command::new("xcrun")
        .args(["--find", "libtool"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let libtool = String::from_utf8_lossy(&output.stdout);
    let libtool = std::path::PathBuf::from(libtool.lines().next()?.trim());
    libtool.is_file().then_some(libtool)
}

pub(super) fn add_cc_specific_flags(cc: &CC, buf: &mut Vec<String>) {
    // The flags are required by the generated C runtime. See:
    // https://github.com/moonbitlang/core/issues/1594#issuecomment-2649652455
    if cc.is_full_featured_gcc_like() {
        buf.push("-fwrapv".to_string());
        buf.push("-fno-strict-aliasing".to_string());
        // Apple clang is usually detected as SystemCC on macOS.
        if matches!(cc.cc_kind, CCKind::Clang)
            || (cfg!(target_os = "macos") && matches!(cc.cc_kind, CCKind::SystemCC))
        {
            buf.push("-Wno-unused-value".to_string());
        }
    }
}

pub(super) fn add_linker_common_libraries<P: AsRef<Path>>(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &LinkerConfig<P>,
) {
    if cc.is_gcc_like() {
        if cc.should_link_libm() {
            buf.push("-lm".to_string());
        }
        if let Some(dyn_lib_path) = config.link_shared_runtime.as_ref() {
            buf.push("-lruntime".to_string());
            buf.push(format!("-Wl,-rpath,{}", dyn_lib_path.as_ref().display()));
        }
    }
}

pub(super) fn add_cc_common_libraries(cc: &CC, buf: &mut Vec<String>, config: &CCConfig) {
    if cc.should_link_libm() && config.output_ty != OutputType::Object {
        buf.push("-lm".to_string());
    }
}
