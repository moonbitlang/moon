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

use expect_test::expect;

use crate::{
    TestDir, moon_cmd,
    util::{check, moon_bin, moonrun_bin, read},
};

// Upstream async has tick-sensitive tests; keep wasm package runs isolated
// from the Rust test harness's package-level concurrency.
static ASYNC_WASM_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
const MOONBIT_ASYNC_CHECK_FD_LEAK: &str = "MOONBIT_ASYNC_CHECK_FD_LEAK";

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .unwrap()
        .to_path_buf()
}

fn prepare_async_wasm_workspace(dir: &TestDir) -> std::path::PathBuf {
    let repo_root = repo_root();
    let async_dir = repo_root.join("third_party/moonbitlang_async");
    let async_member = async_dir
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");

    std::fs::write(
        dir.join("moon.work"),
        read(dir.join("moon.work.template")).replace("@@ASYNC_MEMBER@@", &async_member),
    )
    .unwrap();
    std::fs::copy(dir.join("app/moon.mod.template"), dir.join("app/moon.mod")).unwrap();

    async_dir
}

fn run_async_wasm_package(dir: &TestDir, package: &str) -> String {
    let _guard = ASYNC_WASM_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let moonrun = moonrun_bin();
    let output = moon_cmd(dir)
        .env("MOON_OVERRIDE", moon_bin())
        .env("MOONRUN_OVERRIDE", &moonrun)
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .args([
            "-C",
            "app/main",
            "test",
            "--target",
            "wasm",
            "--package",
            package,
            "--sort-input",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    std::str::from_utf8(&output).unwrap().to_owned()
}

fn run_upstream_async_wasm_tests() {
    let _guard = ASYNC_WASM_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let moonrun = moonrun_bin();
    let async_dir = repo_root().join("third_party/moonbitlang_async");
    let build_dir = async_dir.join("_build");
    std::fs::create_dir_all(&build_dir).expect("failed to create async test build dir");
    let target_dir = tempfile::Builder::new()
        .prefix("moon-test-target-")
        .tempdir_in(&build_dir)
        .expect("failed to create async test target dir");
    let moon = moon_bin();
    let path = std::env::join_paths(
        std::iter::once(
            moon.parent()
                .expect("test moon binary should have a parent directory")
                .to_path_buf(),
        )
        .chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )),
    )
    .expect("failed to put the test moon binary on PATH");
    moon_cmd(&async_dir)
        .env("MOON_OVERRIDE", &moon)
        .env("MOONRUN_OVERRIDE", &moonrun)
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .env("PATH", path)
        .arg("--target-dir")
        .arg(target_dir.path())
        .args(["test", "--target", "wasm"])
        .assert()
        .success();
}

#[test]
fn test_async_wasm_workspace_timer() {
    let dir = TestDir::new("moon_test/async_wasm_workspace_timer");
    prepare_async_wasm_workspace(&dir);

    check(
        run_async_wasm_package(&dir, "moon/async_timer_workspace/main"),
        expect![[r#"
            timer resumed
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[test]
fn test_async_wasm_workspace_fs_smoke() {
    let dir = TestDir::new("moon_test/async_wasm_workspace_fs");
    prepare_async_wasm_workspace(&dir);

    check(
        run_async_wasm_package(&dir, "moon/async_fs_workspace/main"),
        expect![[r#"
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
}

#[test]
#[ignore = "run in CI when Moonrun or upstream async changes"]
fn test_async_wasm_upstream() {
    run_upstream_async_wasm_tests();
}
