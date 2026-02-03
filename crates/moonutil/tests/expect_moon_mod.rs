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

use std::path::PathBuf;

use moonutil::common::{TargetBackend, read_module_desc_file_in_dir, read_module_from_dsl};
use semver::{Version, VersionReq};

fn fixture_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn read_module_from_dsl_basic() {
    let path = fixture_dir("module_dsl_only").join("moon.mod");
    let module = read_module_from_dsl(&path).expect("read moon.mod");

    assert_eq!(module.name, "example/dsl_only");
    assert_eq!(module.version, Some(Version::parse("0.1.0").unwrap()));
    assert_eq!(module.source.as_deref(), Some("src"));
    assert_eq!(module.preferred_target, Some(TargetBackend::WasmGC));
    assert_eq!(module.warn_list.as_deref(), Some("-unused-deprecated"));

    let dep = module.deps.get("moonbitlang/x").expect("dep exists");
    assert_eq!(dep.version, VersionReq::parse("0.4.6").unwrap());
}

#[test]
fn read_module_desc_prefers_dsl() {
    let dir = fixture_dir("module_both");
    let module = read_module_desc_file_in_dir(&dir).expect("read module descriptor");
    assert_eq!(module.name, "example/dsl_prefers");
}
