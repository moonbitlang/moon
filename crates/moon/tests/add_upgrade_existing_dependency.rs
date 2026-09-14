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
    let layout = moonutil::MoonHomeLayout::new(moon_home.to_path_buf());
    let index = layout.registry_index_file(&"example/dep".into());
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
