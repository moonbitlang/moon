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

use std::path::{Path, PathBuf};

fn moon_bin() -> PathBuf {
    snapbox::cargo_bin!("moon").to_owned()
}

fn run_moon(dir: &Path, moon_home: &Path) -> snapbox::cmd::Command {
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(dir)
        .env("MOON_HOME", moon_home)
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
}

fn write_registry_index(moon_home: &Path) {
    let index_dir = moon_home.join("registry").join("index");
    let index = index_dir.join("user").join("example").join("dep.index");
    std::fs::create_dir_all(index.parent().unwrap()).unwrap();
    std::fs::write(
        index,
        r#"{"name":"example/dep","version":"0.2.0"}
{"name":"example/dep","version":"0.3.0"}
"#,
    )
    .unwrap();
}

#[test]
fn moon_add_existing_dependency_in_moon_mod_is_noop() {
    let project = tempfile::tempdir().unwrap();
    let moon_home = tempfile::tempdir().unwrap();
    let manifest = project.path().join("moon.mod");
    let original = r#"name = "test/add_existing"

version = "0.0.1"

import {
  "example/dep@0.1.0",
}
"#;
    std::fs::write(&manifest, original).unwrap();

    run_moon(project.path(), moon_home.path())
        .args(["add", "--no-update", "example/dep@0.2.0"])
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq("Warning: dependency `example/dep` already exists, `moon add` will not update it. To update the dependency, run `moon add --upgrade example/dep@<version>` or `moon add --upgrade example/dep` for the latest version.\n");

    assert_eq!(std::fs::read_to_string(manifest).unwrap(), original);
}

#[test]
fn moon_add_upgrade_dependency_to_explicit_version_in_moon_mod_succeeds() {
    let project = tempfile::tempdir().unwrap();
    let moon_home = tempfile::tempdir().unwrap();
    let manifest = project.path().join("moon.mod");
    std::fs::write(
        &manifest,
        r#"name = "test/update_existing"

version = "0.0.1"

import {
  "example/dep@0.1.0",
}
"#,
    )
    .unwrap();

    run_moon(project.path(), moon_home.path())
        .args(["add", "--upgrade", "example/dep@0.2.0"])
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq("");

    assert_eq!(
        std::fs::read_to_string(manifest).unwrap(),
        r#"name = "test/update_existing"

version = "0.0.1"

import {
  "example/dep@0.2.0",
}
"#
    );
}

#[test]
fn moon_add_upgrade_dependency_to_latest_in_moon_mod_json_succeeds() {
    let project = tempfile::tempdir().unwrap();
    let moon_home = tempfile::tempdir().unwrap();
    let registry_base = tempfile::tempdir().unwrap();
    write_registry_index(moon_home.path());
    let manifest = project.path().join("moon.mod.json");
    std::fs::write(
        &manifest,
        r#"{
  "name": "test/update_existing",
  "version": "0.0.1",
  "deps": {
    "example/dep": "0.1.0"
  }
}
"#,
    )
    .unwrap();

    run_moon(project.path(), moon_home.path())
        .env("MOONCAKES_REGISTRY", registry_base.path())
        .args(["add", "-u", "example/dep"])
        .assert()
        .success();

    assert!(
        std::fs::read_to_string(manifest)
            .unwrap()
            .contains(r#""example/dep": "0.3.0""#)
    );
}
