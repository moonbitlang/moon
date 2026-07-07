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

//! Version utilities
use anyhow::bail;
use semver::{Comparator, Op, Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;

/// Converts a version into a semver comparator
pub fn as_comparator(version: Version, op: Op) -> Comparator {
    Comparator {
        op,
        major: version.major,
        minor: Some(version.minor),
        patch: Some(version.patch),
        pre: version.pre,
    }
}

/// Converts a version into a caret comparator
pub fn as_caret_comparator(version: Version) -> Comparator {
    as_comparator(version, Op::Caret)
}

/// Converts a version into a caret version requirement
pub fn as_caret_version_req(version: Version) -> VersionReq {
    VersionReq {
        comparators: vec![as_caret_comparator(version)],
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VersionItems {
    pub items: Vec<VersionItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VersionItem {
    pub name: String,
    pub version: String,
    pub path: Option<String>,
}

pub fn get_cargo_pkg_version() -> String {
    env!("CARGO_PKG_VERSION").into()
}

pub fn get_moon_version() -> String {
    format!(
        "{} ({} {})",
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_GIT_SHA"),
        std::env!("VERGEN_BUILD_DATE")
    )
}

pub fn get_moonc_version() -> anyhow::Result<String> {
    get_program_version_ex(&*crate::BINARIES.moonc, "-v")
}

pub fn get_moonrun_version() -> anyhow::Result<String> {
    get_program_version(&*crate::BINARIES.moonrun)
}

pub fn get_program_version(program: impl AsRef<OsStr>) -> anyhow::Result<String> {
    get_program_version_ex(program, "--version")
}

fn get_program_version_ex(
    program: impl AsRef<OsStr>,
    option: impl AsRef<OsStr>,
) -> anyhow::Result<String> {
    let program = program.as_ref();
    let output = std::process::Command::new(program).arg(option).output();
    match output {
        Ok(output) => {
            if output.status.success() {
                Ok(std::str::from_utf8(&output.stdout)?.trim().to_string())
            } else {
                bail!(
                    "failed to get {program:?} version: {}",
                    std::str::from_utf8(&output.stderr)?
                );
            }
        }
        Err(e) => bail!("failed to get {program:?} version: {e}"),
    }
}

#[test]
fn test_get_version() {
    let v = get_moon_version();
    println!("moon_version: {v}");
    assert!(!v.is_empty());
    let v = get_moonc_version().unwrap();
    println!("moonc_version: {v}");
    assert!(!v.is_empty());
}
