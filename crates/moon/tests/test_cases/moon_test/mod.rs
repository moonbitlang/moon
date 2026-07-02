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

mod patch;
#[cfg(unix)]
mod use_cc_for_native_release;
mod with_cfg;

use expect_test::expect_file;

use crate::dry_run_utils::assert_lines_in_order;

use super::*;

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .unwrap()
        .to_path_buf()
}

// Upstream async has tick-sensitive tests; keep wasm package runs isolated
// from the Rust test harness's package-level concurrency.
static ASYNC_WASM_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
const MOONBIT_ASYNC_CHECK_FD_LEAK: &str = "MOONBIT_ASYNC_CHECK_FD_LEAK";

fn prepare_async_wasm_workspace(dir: &TestDir) -> std::path::PathBuf {
    let repo_root = repo_root();
    let async_dir = repo_root.join("third_party/moonbitlang_async");
    let async_member = async_dir
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");

    std::fs::write(
        dir.join("moon.work"),
        crate::util::read(dir.join("moon.work.template"))
            .replace("@@ASYNC_MEMBER@@", &async_member),
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

fn run_upstream_async_wasm_package(package: &str) -> String {
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
    let output = moon_cmd(&async_dir)
        .env("MOON_OVERRIDE", moon_bin())
        .env("MOONRUN_OVERRIDE", &moonrun)
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .arg("--target-dir")
        .arg(target_dir.path())
        .args([
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

#[test]
fn test_moon_test_succ() {
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("NO_COLOR", "1") };
    let dir = TestDir::new("moon_test/succ");
    check(
        get_stdout(&dir, ["test", "-v", "--sort-input", "--no-parallelize"]),
        expect![[r#"
            [moontest] test lib/hello_wbtest.mbt:1 (#0) ok
            [moontest] test lib2/hello_wbtest.mbt:1 (#0) ok
            [moontest] test lib2/nested/lib_wbtest.mbt:1 (#0) ok
            [moontest] test lib2/nested/lib_wbtest.mbt:7 (#1) ok
            [moontest] test lib3/hello_wbtest.mbt:1 (#0) ok
            [moontest] test lib4/hello_wbtest.mbt:1 (#0) ok
            Total tests: 6, passed: 6, failed: 0.
        "#]],
    );
}

#[test]
#[cfg(not(windows))]
fn test_moon_test_succ_llvm() {
    let dir = TestDir::new("moon_test/succ");
    let output = moon_cmd(&dir)
        .env("MOON_OVERRIDE", moon_bin())
        .args([
            "test",
            "--target",
            "llvm",
            "--sort-input",
            "--no-parallelize",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    check(
        std::str::from_utf8(&output).unwrap(),
        expect![[r#"
            Total tests: 6, passed: 6, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_hello_exec() {
    let dir = TestDir::new("moon_test/hello_exec");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "-v"]),
        expect![[r#"
            this is lib test
            [moonbitlang/hello] test lib/hello_wbtest.mbt:1 (#0) ok
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    assert_dry_run_graph(
        &dir,
        ["test", "--dry-run", "--debug", "--sort-input"],
        expect_file!["moon_test_hello_exec_graph.jsonl.snap"],
    );
}

#[test]
fn test_moon_test_hello_exec_fntest() {
    let dir = TestDir::new("moon_test/hello_exec_fntest");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            init in main/main.mbt
        "#]],
    );

    assert_dry_run_graph(
        &dir,
        ["test", "-v", "--dry-run", "--sort-input"],
        expect_file!["moon_test_hello_exec_fntest_graph.jsonl.snap"],
    );

    let test_out = get_stdout(&dir, ["test", "-v", "--sort-input", "--no-parallelize"]);
    assert_lines_in_order(
        &test_out,
        r"
test in lib/hello.mbt
test in lib/hello_test.mbt
Total tests: 2, passed: 2, failed: 0.
    ",
    );
    assert_lines_in_order(
        &test_out,
        r"
[moonbitlang/hello] test lib/hello.mbt:5 (#0) ok
[moonbitlang/hello] test lib/hello_wbtest.mbt:1 (#0) ok
    ",
    );
}

#[test]
fn test_moon_test_hello_lib() {
    let dir = TestDir::new("moon_test/hello_lib");
    check(
        get_stdout(&dir, ["test", "-v"]),
        expect![[r#"
            [moonbitlang/hello] test lib/hello_wbtest.mbt:1 (#0) ok
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    )
}

#[test]
fn test_moon_test_runs_from_module_root() {
    let dir = TestDir::new("moon_test/test_cwd");
    let lib_dir = dir.join("lib");

    check(
        get_stdout(
            &lib_dir,
            [
                "--manifest-path",
                "../moon.mod.json",
                "test",
                "--target",
                "js",
                "--no-parallelize",
                "--sort-input",
            ],
        ),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_workspace_members_run_from_module_root() {
    let dir = TestDir::new("moon_test/workspace_cwd");
    let spawn_dir = dir.join("spawn");
    std::fs::create_dir(&spawn_dir).expect("failed to create spawn directory");

    check(
        get_stdout(
            &spawn_dir,
            [
                "--manifest-path",
                "../moon.work",
                "test",
                "--target",
                "js",
                "--no-parallelize",
                "--sort-input",
            ],
        ),
        expect![[r#"
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
}

#[test]
fn test_zombie_child_process() {
    use super::process::{
        read_pid_file, terminate_child, terminate_pid, wait_for_child_exit, wait_for_pid_exit,
    };
    use std::process::Stdio;
    use std::thread;
    use std::time::{Duration, Instant};

    let dir = TestDir::new("moon_test/zombie_child");
    let child_pid_file = dir.join("test_child_pid.txt");

    let build_output = moon_process_cmd(&dir)
        .args(["test", "--target", "js", "--no-parallelize", "--build-only"])
        .output()
        .expect("Failed to build zombie child test fixture");
    assert!(
        build_output.status.success(),
        "failed to build zombie child test fixture\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build_output.stdout),
        String::from_utf8_lossy(&build_output.stderr)
    );

    // Spawn moon test in background
    let mut moon_child = moon_process_cmd(&dir)
        .args(["test", "--target", "js", "--no-parallelize"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn moon test");

    // Wait for the test executable to publish its PID.
    let start = Instant::now();
    let child_pid = loop {
        if let Ok(pid) = read_pid_file(&child_pid_file) {
            break pid;
        }
        if let Some(status) = moon_child
            .try_wait()
            .expect("Failed to poll moon test process")
        {
            let output = moon_child
                .wait_with_output()
                .expect("Failed to collect moon test output");
            panic!(
                "moon test exited before writing child PID: {status}\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        thread::sleep(Duration::from_millis(100));
        if start.elapsed() > Duration::from_secs(60) {
            let _ = moon_child.kill();
            let output = moon_child
                .wait_with_output()
                .expect("Failed to collect moon test output");
            panic!(
                "Timeout waiting for child PID to be written\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    };

    // Terminate the moon process (simulating the scenario in example/script.js)
    terminate_child(&mut moon_child);

    // When moon is killed, all child processes (moonrun/node) should also be terminated.
    // This is verified by checking that the test executable PID exits after
    // the parent `moon` process has been terminated.
    if !wait_for_child_exit(&mut moon_child, Duration::from_secs(5)) {
        let _ = moon_child.kill();
        let _ = moon_child.wait();
        terminate_pid(child_pid);
        panic!("moon process did not exit after termination request");
    }
    if !wait_for_pid_exit(child_pid, Duration::from_secs(5)) {
        terminate_pid(child_pid);
        panic!(
            "Child processes (moonrun/node) are not terminated when moon is killed. \
        The test executable process with PID {child_pid} is still alive after timeout. \
        Moon should properly propagate termination signals to all child processes."
        );
    }
}

#[test]
fn test_moon_test_with_local_dep() {
    let dir = TestDir::new("moon_test/with_local_deps");
    check(
        get_stdout(&dir, ["test", "-v", "--frozen"]),
        expect![[r#"
            [hello31] test lib/hello_wbtest.mbt:1 (#0) ok
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(&dir, ["run", "main", "--frozen"]),
        expect![[r#"
            hello from mooncake
            hello from mooncake2
        "#]],
    );
    // Run moon info
    moon_cmd(&dir).args(["info", "--frozen"]).assert().success();
    // Check directory structure by listing all files
    let root_dir = dir.as_ref().to_owned();
    let dir = WalkDir::new(&dir)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|e| e.path().strip_prefix(&root_dir).unwrap().to_owned())
        // Convert to string and join with newline
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let joined = dir.join("\n").replace('\\', "/"); // Normalize path separator
    check(
        &joined,
        expect![[r#"

            .gitignore
            _build
            _build/.moon-lock
            _build/wasm-gc
            _build/wasm-gc/debug
            _build/wasm-gc/debug/build
            _build/wasm-gc/debug/build/.mooncakes
            _build/wasm-gc/debug/build/.mooncakes/lijunchen
            _build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake
            _build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake/lib
            _build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake/lib/lib.core
            _build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake/lib/lib.mi
            _build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake/mooncake.core
            _build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake/mooncake.mi
            _build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake2
            _build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake2/lib
            _build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake2/lib/lib.core
            _build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake2/lib/lib.mi
            _build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake2/mooncake2.core
            _build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake2/mooncake2.mi
            _build/wasm-gc/debug/build/all_pkgs.json
            _build/wasm-gc/debug/build/build.moon_db
            _build/wasm-gc/debug/build/lib
            _build/wasm-gc/debug/build/lib/lib.core
            _build/wasm-gc/debug/build/lib/lib.mi
            _build/wasm-gc/debug/build/main
            _build/wasm-gc/debug/build/main/main.core
            _build/wasm-gc/debug/build/main/main.mi
            _build/wasm-gc/debug/build/main/main.wasm
            _build/wasm-gc/debug/build/main/main.wasm.map
            _build/wasm-gc/debug/check
            _build/wasm-gc/debug/check/.mooncakes
            _build/wasm-gc/debug/check/.mooncakes/lijunchen
            _build/wasm-gc/debug/check/.mooncakes/lijunchen/mooncake
            _build/wasm-gc/debug/check/.mooncakes/lijunchen/mooncake/lib
            _build/wasm-gc/debug/check/.mooncakes/lijunchen/mooncake/lib/lib.ast
            _build/wasm-gc/debug/check/.mooncakes/lijunchen/mooncake/lib/lib.mi
            _build/wasm-gc/debug/check/.mooncakes/lijunchen/mooncake/lib/lib.typechecked
            _build/wasm-gc/debug/check/.mooncakes/lijunchen/mooncake/mooncake.ast
            _build/wasm-gc/debug/check/.mooncakes/lijunchen/mooncake/mooncake.mi
            _build/wasm-gc/debug/check/.mooncakes/lijunchen/mooncake/mooncake.typechecked
            _build/wasm-gc/debug/check/.mooncakes/lijunchen/mooncake2
            _build/wasm-gc/debug/check/.mooncakes/lijunchen/mooncake2/lib
            _build/wasm-gc/debug/check/.mooncakes/lijunchen/mooncake2/lib/lib.ast
            _build/wasm-gc/debug/check/.mooncakes/lijunchen/mooncake2/lib/lib.mi
            _build/wasm-gc/debug/check/.mooncakes/lijunchen/mooncake2/lib/lib.typechecked
            _build/wasm-gc/debug/check/.mooncakes/lijunchen/mooncake2/mooncake2.ast
            _build/wasm-gc/debug/check/.mooncakes/lijunchen/mooncake2/mooncake2.mi
            _build/wasm-gc/debug/check/.mooncakes/lijunchen/mooncake2/mooncake2.typechecked
            _build/wasm-gc/debug/check/all_pkgs.json
            _build/wasm-gc/debug/check/check.moon_db
            _build/wasm-gc/debug/check/lib
            _build/wasm-gc/debug/check/lib/lib.ast
            _build/wasm-gc/debug/check/lib/lib.mbti
            _build/wasm-gc/debug/check/lib/lib.mi
            _build/wasm-gc/debug/check/lib/lib.typechecked
            _build/wasm-gc/debug/check/main
            _build/wasm-gc/debug/check/main/main.ast
            _build/wasm-gc/debug/check/main/main.mbti
            _build/wasm-gc/debug/check/main/main.mi
            _build/wasm-gc/debug/check/main/main.typechecked
            _build/wasm-gc/debug/test
            _build/wasm-gc/debug/test/.mooncakes
            _build/wasm-gc/debug/test/.mooncakes/lijunchen
            _build/wasm-gc/debug/test/.mooncakes/lijunchen/mooncake
            _build/wasm-gc/debug/test/.mooncakes/lijunchen/mooncake/lib
            _build/wasm-gc/debug/test/.mooncakes/lijunchen/mooncake/lib/lib.core
            _build/wasm-gc/debug/test/.mooncakes/lijunchen/mooncake/lib/lib.mi
            _build/wasm-gc/debug/test/.mooncakes/lijunchen/mooncake/mooncake.core
            _build/wasm-gc/debug/test/.mooncakes/lijunchen/mooncake/mooncake.mi
            _build/wasm-gc/debug/test/.mooncakes/lijunchen/mooncake2
            _build/wasm-gc/debug/test/.mooncakes/lijunchen/mooncake2/lib
            _build/wasm-gc/debug/test/.mooncakes/lijunchen/mooncake2/lib/lib.core
            _build/wasm-gc/debug/test/.mooncakes/lijunchen/mooncake2/lib/lib.mi
            _build/wasm-gc/debug/test/.mooncakes/lijunchen/mooncake2/mooncake2.core
            _build/wasm-gc/debug/test/.mooncakes/lijunchen/mooncake2/mooncake2.mi
            _build/wasm-gc/debug/test/all_pkgs.json
            _build/wasm-gc/debug/test/lib
            _build/wasm-gc/debug/test/lib/__blackbox_test_info.json
            _build/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt
            _build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt
            _build/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt
            _build/wasm-gc/debug/test/lib/__internal_test_info.json
            _build/wasm-gc/debug/test/lib/__whitebox_test_info.json
            _build/wasm-gc/debug/test/lib/lib.blackbox_test.core
            _build/wasm-gc/debug/test/lib/lib.blackbox_test.wasm
            _build/wasm-gc/debug/test/lib/lib.blackbox_test.wasm.map
            _build/wasm-gc/debug/test/lib/lib.core
            _build/wasm-gc/debug/test/lib/lib.internal_test.core
            _build/wasm-gc/debug/test/lib/lib.internal_test.wasm
            _build/wasm-gc/debug/test/lib/lib.internal_test.wasm.map
            _build/wasm-gc/debug/test/lib/lib.mi
            _build/wasm-gc/debug/test/lib/lib.whitebox_test.core
            _build/wasm-gc/debug/test/lib/lib.whitebox_test.wasm
            _build/wasm-gc/debug/test/lib/lib.whitebox_test.wasm.map
            _build/wasm-gc/debug/test/main
            _build/wasm-gc/debug/test/main/__blackbox_test_info.json
            _build/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt
            _build/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt
            _build/wasm-gc/debug/test/main/__internal_test_info.json
            _build/wasm-gc/debug/test/main/main.blackbox_test.core
            _build/wasm-gc/debug/test/main/main.blackbox_test.wasm
            _build/wasm-gc/debug/test/main/main.blackbox_test.wasm.map
            _build/wasm-gc/debug/test/main/main.core
            _build/wasm-gc/debug/test/main/main.internal_test.core
            _build/wasm-gc/debug/test/main/main.internal_test.wasm
            _build/wasm-gc/debug/test/main/main.internal_test.wasm.map
            _build/wasm-gc/debug/test/main/main.mi
            _build/wasm-gc/debug/test/test.moon_db
            lib
            lib/hello.mbt
            lib/hello_wbtest.mbt
            lib/moon.pkg.json
            lib/pkg.generated.mbti
            main
            main/main.mbt
            main/moon.pkg.json
            main/pkg.generated.mbti
            mods
            mods/lijunchen
            mods/lijunchen/mooncake
            mods/lijunchen/mooncake/lib
            mods/lijunchen/mooncake/lib/hello.mbt
            mods/lijunchen/mooncake/lib/hello_wbtest.mbt
            mods/lijunchen/mooncake/lib/moon.pkg.json
            mods/lijunchen/mooncake/moon.mod.json
            mods/lijunchen/mooncake/moon.pkg.json
            mods/lijunchen/mooncake/top.mbt
            mods/lijunchen/mooncake2
            mods/lijunchen/mooncake2/moon.mod.json
            mods/lijunchen/mooncake2/src
            mods/lijunchen/mooncake2/src/lib
            mods/lijunchen/mooncake2/src/lib/hello.mbt
            mods/lijunchen/mooncake2/src/lib/hello_wbtest.mbt
            mods/lijunchen/mooncake2/src/lib/moon.pkg.json
            mods/lijunchen/mooncake2/src/moon.pkg.json
            mods/lijunchen/mooncake2/src/top.mbt
            moon.mod.json"#]],
    );
}

#[test]
fn test_pkg_source_in() {
    let dir = TestDir::new("moon_test/with_local_deps");
    let out = get_stdout(&dir, ["build", "--dry-run", "--sort-input", "--frozen"]);
    check(
        &out,
        expect![[r#"
            moonc build-package ./mods/lijunchen/mooncake2/src/lib/hello.mbt -w -a -o ./_build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake2/lib/lib.core -pkg lijunchen/mooncake2/lib -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources lijunchen/mooncake2/lib:./mods/lijunchen/mooncake2/src/lib -target wasm-gc -g -O0 -source-map -workspace-path ./mods/lijunchen/mooncake2 -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./mods/lijunchen/mooncake2/src/top.mbt -w -a -o ./_build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake2/mooncake2.core -pkg lijunchen/mooncake2 -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake2/lib/lib.mi:lib -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources lijunchen/mooncake2:./mods/lijunchen/mooncake2/src -target wasm-gc -g -O0 -source-map -workspace-path ./mods/lijunchen/mooncake2 -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./mods/lijunchen/mooncake/lib/hello.mbt -w -a -o ./_build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake/lib/lib.core -pkg lijunchen/mooncake/lib -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources lijunchen/mooncake/lib:./mods/lijunchen/mooncake/lib -target wasm-gc -g -O0 -source-map -workspace-path ./mods/lijunchen/mooncake -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./mods/lijunchen/mooncake/top.mbt -w -a -o ./_build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake/mooncake.core -pkg lijunchen/mooncake -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake/lib/lib.mi:lib -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources lijunchen/mooncake:./mods/lijunchen/mooncake -target wasm-gc -g -O0 -source-map -workspace-path ./mods/lijunchen/mooncake -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg hello31/lib -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources hello31/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg hello31/main -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -i ./_build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake/mooncake.mi:mooncake -i ./_build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake2/mooncake2.mi:mooncake2 -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources hello31/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake/lib/lib.core ./_build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake/mooncake.core ./_build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake2/lib/lib.core ./_build/wasm-gc/debug/build/.mooncakes/lijunchen/mooncake2/mooncake2.core ./_build/wasm-gc/debug/build/main/main.core -main hello31/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello31/lib:./lib -pkg-sources lijunchen/mooncake/lib:./mods/lijunchen/mooncake/lib -pkg-sources lijunchen/mooncake:./mods/lijunchen/mooncake -pkg-sources lijunchen/mooncake2/lib:./mods/lijunchen/mooncake2/src/lib -pkg-sources lijunchen/mooncake2:./mods/lijunchen/mooncake2/src -pkg-sources hello31/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
        "#]],
    );
    assert!(out.contains("lijunchen/mooncake:./mods/lijunchen/mooncake"));
    assert!(out.contains("lijunchen/mooncake2:./mods/lijunchen/mooncake2/src"));
}
#[test]
fn test_moon_test_no_entry_warning() {
    let dir = TestDir::new("moon_test/no_entry_warning");

    let out = moon_cmd(&dir)
        .args(["test"])
        .assert()
        .success()
        .get_output()
        .stderr
        .to_owned();

    check(
        std::str::from_utf8(&out).unwrap(),
        expect![[r#"
            Warning: no test entry found.
        "#]],
    );
}

#[test]
#[ignore]
fn test_generate_test_driver_incremental() {
    let dir = TestDir::new("moon_test/hello_lib");

    get_stdout(&dir, ["test", "--package", "moonbitlang/hello/lib"]);
    let driver_file =
        dir.join("_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt");
    assert!(driver_file.exists());

    let time_1 = driver_file.metadata().unwrap().modified().unwrap();

    get_stdout(
        &dir,
        [
            "test",
            "--package",
            "moonbitlang/hello/lib",
            "--file",
            "hello_wbtest.mbt",
        ],
    );
    let time_2 = driver_file.metadata().unwrap().modified().unwrap();

    assert!(time_1 == time_2);

    get_stdout(
        &dir,
        [
            "test",
            "--package",
            "moonbitlang/hello/lib",
            "--file",
            "hello_wbtest.mbt",
            "--index",
            "0",
        ],
    );
    let time_3 = driver_file.metadata().unwrap().modified().unwrap();

    assert!(time_2 == time_3);

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(dir.join("lib/hello.mbt"))
        .unwrap();
    file.write_all(b"\n").unwrap();

    get_stdout(
        &dir,
        [
            "test",
            "--package",
            "moonbitlang/hello/lib",
            "--file",
            "hello_wbtest.mbt",
            "--index",
            "0",
        ],
    );
    let time_4 = driver_file.metadata().unwrap().modified().unwrap();

    assert!(time_3 != time_4);
}

#[test]
fn test_async_test_inline() {
    let dir = TestDir::new("moon_test");

    let out1 = get_stdout(&dir, ["-C", "async_test_inline", "test"]);
    check(
        &out1,
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    )
}

#[test]
fn test_async_test() {
    let dir = TestDir::new("moon_test");
    let out1 = get_stdout(
        &dir,
        [
            "-C",
            "async_test",
            "test",
            "--package",
            "moon/test_async_test",
            "--file",
            "async_test.mbt",
            "--index",
            "0",
        ],
    );
    check(
        &out1,
        expect![[r#"
        Total tests: 1, passed: 1, failed: 0.
    "#]],
    );
    let out2 = get_err_stdout(
        &dir,
        [
            "-C",
            "async_test",
            "test",
            "--package",
            "moon/test_async_test",
            "--file",
            "async_test.mbt",
            "--index",
            "1",
        ],
    );
    let last_line = out2.lines().last().unwrap_or("");
    check(last_line, expect!["Total tests: 1, passed: 0, failed: 1."])
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
fn test_async_wasm_upstream_src_package() {
    check(
        run_upstream_async_wasm_package("moonbitlang/async"),
        expect![[r#"
            Total tests: 91, passed: 91, failed: 0.
        "#]],
    );
}

#[test]
fn test_async_wasm_upstream_aqueue_package() {
    check(
        run_upstream_async_wasm_package("moonbitlang/async/aqueue"),
        expect![[r#"
            Total tests: 52, passed: 52, failed: 0.
        "#]],
    );
}

#[test]
fn test_async_wasm_upstream_cond_var_package() {
    check(
        run_upstream_async_wasm_package("moonbitlang/async/cond_var"),
        expect![[r#"
            Total tests: 8, passed: 8, failed: 0.
        "#]],
    );
}

#[test]
fn test_async_wasm_upstream_semaphore_package() {
    check(
        run_upstream_async_wasm_package("moonbitlang/async/semaphore"),
        expect![[r#"
            Total tests: 12, passed: 12, failed: 0.
        "#]],
    );
}

#[test]
fn test_async_wasm_upstream_fs_package() {
    check(
        run_upstream_async_wasm_package("moonbitlang/async/fs"),
        expect![[r#"
        Total tests: 31, passed: 31, failed: 0.
        "#]],
    );
}

#[test]
fn test_async_wasm_upstream_pipe_package() {
    check(
        run_upstream_async_wasm_package("moonbitlang/async/pipe"),
        expect![[r#"
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );
}

#[test]
fn test_async_wasm_upstream_socket_package() {
    check(
        run_upstream_async_wasm_package("moonbitlang/async/socket"),
        expect![[r#"
            Total tests: 24, passed: 24, failed: 0.
        "#]],
    );
}

#[test]
fn test_max_concurrent_tests() {
    let dir = TestDir::new("moon_test");
    let out1 = get_stdout(
        &dir,
        [
            "-C",
            "max_concurrent_tests",
            "test",
            "-p",
            "moon/test_async_test/with_limit",
        ],
    );
    check(
        &out1,
        expect![[r#"
            test 1 msg 1
            test 1 msg 2
            test 2 msg 1
            test 2 msg 2
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
    let out2 = get_stdout(
        &dir,
        [
            "-C",
            "max_concurrent_tests",
            "test",
            "-p",
            "moon/test_async_test/no_limit",
        ],
    );
    check(
        &out2,
        expect![[r#"
            test 1 msg 1
            test 2 msg 1
            test 1 msg 2
            test 2 msg 2
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
}

#[test]
fn test_doctest_without_bbtest_file() {
    let dir = TestDir::new("moon_test/doctest_without_bbtest");

    let out1 = get_stdout(&dir, ["test"]);
    check(
        &out1,
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    )
}
