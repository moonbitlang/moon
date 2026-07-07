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

use crate::mooncakes::ModuleName;

pub const MOON_MOD: &str = "moon.mod";
pub const MOON_MOD_JSON: &str = "moon.mod.json";
pub const MOON_PKG_JSON: &str = "moon.pkg.json";
pub const MOON_WORK: &str = "moon.work";
pub const MOON_WORK_ENV: &str = "MOON_WORK";
pub const MOON_NO_WORKSPACE: &str = "MOON_NO_WORKSPACE";
pub const MOON_PKG: &str = "moon.pkg";
pub const MBTI_GENERATED: &str = "pkg.generated.mbti";
pub const MBTI_USER_WRITTEN: &str = "pkg.mbti";
pub const MOONBITLANG_CORE: &str = "moonbitlang/core";
pub const MOONBITLANG_CORE_BUILTIN: &str = "moonbitlang/core/builtin";
pub const MOONBITLANG_CORE_PRELUDE: &str = "moonbitlang/core/prelude";
pub const MOONBITLANG_COVERAGE: &str = "moonbitlang/core/coverage";
pub const MOONBITLANG_ABORT: &str = "moonbitlang/core/abort";

pub static MOD_NAME_STDLIB: ModuleName = ModuleName {
    username: arcstr::literal!("moonbitlang"),
    unqual: arcstr::literal!("core"),
};

pub const MOON_TEST_DELIMITER_BEGIN: &str = "----- BEGIN MOON TEST RESULT -----";
pub const MOON_TEST_DELIMITER_END: &str = "----- END MOON TEST RESULT -----";

pub const MOON_COVERAGE_DELIMITER_BEGIN: &str = "----- BEGIN MOONBIT COVERAGE -----";
pub const MOON_COVERAGE_DELIMITER_END: &str = "----- END MOONBIT COVERAGE -----";

pub const MOON_LOCK: &str = ".moon-lock";

pub const DEP_PATH: &str = ".mooncakes";

pub const BUILD_DIR: &str = "_build";

pub const IGNORE_DIRS: &[&str] = &[BUILD_DIR, ".git", "node_modules", DEP_PATH];

pub const WATCH_MODE_DIR: &str = "watch";

pub const TEST_INFO_FILE: &str = "test_info.json";

pub const WHITEBOX_TEST_PATCH: &str = "_wbtest.json";
pub const BLACKBOX_TEST_PATCH: &str = "_test.json";

pub const DOT_MBT_DOT_MD: &str = ".mbt.md";
pub const DOT_MBTP: &str = ".mbtp";
pub const DOT_MBL: &str = ".mbl";
pub const DOT_MBY: &str = ".mby";

pub const MOON_BIN_DIR: &str = "__moonbin__";

pub const MOONCAKE_BIN: &str = "$mooncake_bin";
pub const MOD_DIR: &str = "$mod_dir";
pub const PKG_DIR: &str = "$pkg_dir";

pub const SINGLE_FILE_TEST_PACKAGE: &str = "moon/test/single";
pub const SINGLE_FILE_TEST_MODULE: &str = "moon/test";

pub const SUB_PKG_POSTFIX: &str = "_sub";

pub const PRELUDE_PROOF_DIR: &str = "prelude_proof";

pub const O_EXT: &str = if cfg!(windows) { "obj" } else { "o" };
#[allow(unused)]
pub const DYN_EXT: &str = if cfg!(windows) {
    "dll"
} else if cfg!(target_os = "macos") {
    "dylib"
} else {
    "so"
};

pub const A_EXT: &str = if cfg!(windows) { "lib" } else { "a" };

pub fn is_moon_pkg_exist(dir: &Path) -> bool {
    dir.join(MOON_PKG).exists() || dir.join(MOON_PKG_JSON).exists()
}

pub fn is_moon_pkg(filename: &str) -> bool {
    filename == MOON_PKG || filename == MOON_PKG_JSON
}

pub fn is_moon_mod_exist(dir: &Path) -> bool {
    dir.join(MOON_MOD).exists() || dir.join(MOON_MOD_JSON).exists()
}

pub fn is_moon_mod(filename: &str) -> bool {
    filename == MOON_MOD || filename == MOON_MOD_JSON
}

pub fn is_moon_work(filename: &str) -> bool {
    filename == MOON_WORK
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageSourceFileKind {
    Mbt,
    MbtMd,
    Mbtp,
    Mbl,
    Mby,
}

pub fn package_source_file_kind(filename: &str) -> Option<PackageSourceFileKind> {
    if filename.ends_with(".mbt") {
        Some(PackageSourceFileKind::Mbt)
    } else if filename.ends_with(DOT_MBT_DOT_MD) {
        Some(PackageSourceFileKind::MbtMd)
    } else if filename.ends_with(DOT_MBTP) {
        Some(PackageSourceFileKind::Mbtp)
    } else if filename.ends_with(DOT_MBL) {
        Some(PackageSourceFileKind::Mbl)
    } else if filename.ends_with(DOT_MBY) {
        Some(PackageSourceFileKind::Mby)
    } else {
        None
    }
}

pub fn is_watch_relevant_project_file(filename: &str) -> bool {
    package_source_file_kind(filename).is_some()
        || is_moon_pkg(filename)
        || is_moon_mod(filename)
        || is_moon_work(filename)
}

#[test]
fn package_source_file_kind_detects_supported_package_inputs() {
    assert_eq!(
        package_source_file_kind("main.mbt"),
        Some(PackageSourceFileKind::Mbt)
    );
    assert_eq!(
        package_source_file_kind("guide.mbt.md"),
        Some(PackageSourceFileKind::MbtMd)
    );
    assert_eq!(
        package_source_file_kind("proof.mbtp"),
        Some(PackageSourceFileKind::Mbtp)
    );
    assert_eq!(
        package_source_file_kind("lexer.mbl"),
        Some(PackageSourceFileKind::Mbl)
    );
    assert_eq!(
        package_source_file_kind("parser.mby"),
        Some(PackageSourceFileKind::Mby)
    );
    assert_eq!(package_source_file_kind("moon.pkg"), None);
}

#[test]
fn watch_relevant_project_file_covers_sources_and_manifests() {
    assert!(is_watch_relevant_project_file("moon.mod"));
    assert!(is_watch_relevant_project_file("moon.mod.json"));
    assert!(is_watch_relevant_project_file("moon.work"));
    assert!(is_watch_relevant_project_file("moon.pkg"));
    assert!(is_watch_relevant_project_file("moon.pkg.json"));
    assert!(is_watch_relevant_project_file("lexer.mbl"));
    assert!(!is_watch_relevant_project_file("README.md"));
}
