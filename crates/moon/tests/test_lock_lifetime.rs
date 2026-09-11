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

use std::{fs, path::Path};

// Run check synchronously inside the MoonBit test. If the outer command still
// holds the build lock, check times out and the MoonBit assertion fails.
const CHECK: &str = r#"
extern "js" fn check() -> Int =
  #| () => require("node:child_process").spawnSync(
  #|   process.env.MOON_TEST_MOON,
  #|   ["--target-dir", process.env.MOON_TEST_TARGET_DIR,
  #|    "check", process.env.MOON_TEST_CHECK_PATH, "--target", "js", "--quiet"],
  #|   { stdio: "inherit", timeout: 30000 }
  #| ).status ?? -1
"#;

fn moon(root: &Path) -> snapbox::cmd::Command {
    let target = root.join("custom build");
    snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
        .current_dir(root)
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .env("MOON_DEP_CACHE", "off")
        .env("MOON_TEST_MOON", snapbox::cargo_bin!("moon"))
        .env("MOON_TEST_TARGET_DIR", &target)
        .env("MOON_TEST_CHECK_PATH", ".")
        .arg("--target-dir")
        .arg(target)
}

#[test]
fn test_and_bench_allow_check_during_execution() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("moon.mod.json"),
        r#"{"name":"test/lock","preferred-target":"js"}"#,
    )
    .unwrap();
    fs::write(
        root.join("moon.pkg.json"),
        r#"{"import":["moonbitlang/core/bench"]}"#,
    )
    .unwrap();
    fs::write(
        root.join("test.mbt"),
        format!(
            r#"{CHECK}
test {{ assert_eq(check(), 0) }}
test "bench" (b : @bench.T) {{ assert_eq(check(), 0); b.bench(fn() {{ () }}) }}
"#
        ),
    )
    .unwrap();

    for command in ["test", "bench"] {
        moon(root).arg(command).assert().success();
    }
}

#[test]
fn standalone_test_allows_check_during_execution() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("script.mbtx"),
        format!("{CHECK}\ntest {{ assert_eq(check(), 0) }}\n"),
    )
    .unwrap();
    moon(dir.path())
        .env("MOON_TEST_CHECK_PATH", "script.mbtx")
        .args(["test", "script.mbtx", "--target", "js"])
        .assert()
        .success();
}

#[test]
fn test_update_allows_check_on_each_run() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("moon.mod.json"), r#"{"name":"test/lock"}"#).unwrap();
    fs::write(root.join("moon.pkg.json"), "{}").unwrap();
    fs::write(
        root.join("test.mbt"),
        format!(
            r#"{CHECK}
test {{ assert_eq(check(), 0); inspect(1, content="0") }}
"#
        ),
    )
    .unwrap();
    moon(root)
        .args(["test", "--target", "js", "--update"])
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(root.join("test.mbt")).unwrap(),
        format!(
            r#"{CHECK}
test {{ assert_eq(check(), 0); inspect(1, content=(
  #|1
)) }}
"#
        )
    );
}
