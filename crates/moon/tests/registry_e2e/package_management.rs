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

use moon_test_util::registry::{TestPackage, TestRegistry};

use crate::support::{TestDir, moon_cmd, path_with_moon};

#[test]
fn add_remove_install_and_run() {
    let hello_files: &[(&str, &[u8])] = &[
        (
            "moon.mod",
            br#"name = "test/hello"

version = "0.1.0"
"#,
        ),
        ("lib/moon.pkg", b""),
        (
            "lib/hello.mbt",
            br#"pub fn hello() -> String {
  "Hello, world!"
}
"#,
        ),
    ];
    let hello2_files: &[(&str, &[u8])] = &[
        (
            "moon.mod",
            br#"name = "test/hello2"

version = "0.1.0"

import {
  "test/hello@0.1.0",
}
"#,
        ),
        (
            "lib/moon.pkg",
            br#"import {
  "test/hello/lib",
}
"#,
        ),
        (
            "lib/hello.mbt",
            br#"pub fn hello2() -> String {
  @lib.hello() + "Hello, world2!"
}
"#,
        ),
    ];
    let registry = TestRegistry::with_packages(&[
        TestPackage::new("test/hello", "0.1.0", hello_files),
        TestPackage::new("test/hello2", "0.1.0", hello2_files)
            .with_dependencies(&[("test/hello", "0.1.0")]),
    ]);
    let dir = TestDir::new_empty();
    std::fs::write(
        dir.join("moon.mod"),
        r#"name = "test/root"

version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("main")).unwrap();
    std::fs::write(
        dir.join("main/moon.pkg"),
        r#"import {
  "test/hello2/lib",
}

options(
  "is-main": true,
)
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("main/main.mbt"),
        r#"fn main {
  println(@lib.hello2())
}
"#,
    )
    .unwrap();

    moon_cmd(&dir)
        .envs(registry.envs())
        .arg("update")
        .assert()
        .success();
    moon_cmd(&dir)
        .envs(registry.envs())
        .args(["add", "test/hello2@0.1.0"])
        .assert()
        .success();
    let manifest = std::fs::read_to_string(dir.join("moon.mod")).unwrap();
    assert!(manifest.contains(r#""test/hello2@0.1.0""#));
    assert!(dir.join(".mooncakes/test/hello/moon.mod").is_file());

    moon_cmd(&dir)
        .envs(registry.envs())
        .args(["remove", "test/hello2"])
        .assert()
        .success();
    let manifest = std::fs::read_to_string(dir.join("moon.mod")).unwrap();
    assert!(!manifest.contains("test/hello2"));

    moon_cmd(&dir)
        .envs(registry.envs())
        .args(["add", "test/hello2@0.1.0"])
        .assert()
        .success();
    std::fs::remove_dir_all(dir.join(".mooncakes")).unwrap();
    let install = moon_cmd(&dir)
        .envs(registry.envs())
        .arg("install")
        .assert()
        .success();
    let stderr = String::from_utf8_lossy(&install.get_output().stderr);
    assert!(stderr.contains("Using cached test/hello@0.1.0"));
    assert!(stderr.contains("Using cached test/hello2@0.1.0"));

    std::fs::remove_dir_all(dir.join(".mooncakes")).unwrap();
    let run = moon_cmd(&dir)
        .envs(registry.envs())
        .args(["run", "main"])
        .assert()
        .success()
        .stdout_eq("Hello, world!Hello, world2!\n");
    let stderr = String::from_utf8_lossy(&run.get_output().stderr);
    assert!(
        !stderr.contains("Using cached ") && !stderr.contains("Downloading "),
        "moon run should keep dependency sync quiet by default, got:\n{stderr}"
    );
    registry.assert_used();
}

#[test]
fn update_reclones_for_different_registry_url() {
    let registry = TestRegistry::empty();
    let dir = TestDir::new_empty();

    moon_cmd(&dir)
        .envs(registry.envs())
        .arg("update")
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq("Registry index cloned successfully\nSymbols updated successfully\n");

    let output = std::process::Command::new("git")
        .args(["-C"])
        .arg(registry.moon_home().join("registry/index"))
        .args(["remote", "set-url", "origin", "whatever"])
        .output()
        .unwrap();
    assert!(output.status.success());

    moon_cmd(&dir)
        .envs(registry.envs())
        .arg("update")
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq(
            "Registry index remote does not match the configured URL, re-cloning\nRegistry index re-cloned successfully\nSymbols updated successfully\n",
        );
    moon_cmd(&dir)
        .envs(registry.envs())
        .args(["update", "--quiet"])
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq("");
    registry.assert_used();
}

#[test]
fn postadd_script_can_be_disabled() {
    let manifest = serde_json::json!({
        "name": "test/postadd",
        "version": "1.0.0",
        "scripts": {
            "postadd": "moon tool write-tcc-rsp-file generated.mbt fn generated()->Int{42}"
        }
    })
    .to_string()
    .into_bytes();
    let files: &[(&str, &[u8])] = &[("moon.mod.json", &manifest)];
    let registry = TestRegistry::new("test/postadd", "1.0.0", files);
    let dir = TestDir::new_empty();
    std::fs::write(
        dir.join("moon.mod"),
        r#"name = "test/root"

version = "0.1.0"
"#,
    )
    .unwrap();
    let path = path_with_moon();

    moon_cmd(&dir)
        .envs(registry.envs())
        .arg("update")
        .assert()
        .success();
    moon_cmd(&dir)
        .envs(registry.envs())
        .env("PATH", &path)
        .args(["add", "test/postadd@1.0.0"])
        .assert()
        .success();
    assert!(dir.join(".mooncakes/test/postadd/generated.mbt").is_file());

    moon_cmd(&dir)
        .envs(registry.envs())
        .args(["remove", "test/postadd"])
        .assert()
        .success();
    if dir.join(".mooncakes").exists() {
        std::fs::remove_dir_all(dir.join(".mooncakes")).unwrap();
    }
    moon_cmd(&dir)
        .envs(registry.envs())
        .env("PATH", path)
        .env("MOON_IGNORE_POSTADD", "1")
        .args(["add", "test/postadd@1.0.0"])
        .assert()
        .success();
    assert!(!dir.join(".mooncakes/test/postadd/generated.mbt").exists());
    registry.assert_used();
}
