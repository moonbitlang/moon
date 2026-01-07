mod patch;
mod use_cc_for_native_release;
mod with_cfg;

use expect_test::expect_file;

use crate::{build_graph::compare_graphs, dry_run_utils::assert_lines_in_order};

use super::*;

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
    let graph = dir.join("test_debug_graph.jsonl");
    snap_dry_run_graph(
        &dir,
        ["test", "--dry-run", "--debug", "--sort-input"],
        &graph,
    );
    compare_graphs(
        &graph,
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

    let graph = dir.join("test_graph.jsonl");
    snap_dry_run_graph(&dir, ["test", "-v", "--dry-run", "--sort-input"], &graph);
    compare_graphs(
        &graph,
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
fn test_zombie_child_process() {
    use super::util::moon_bin;
    use std::thread;
    use std::time::{Duration, Instant};

    let dir = TestDir::new("moon_test/zombie_child");
    let lock_file = dir.join("test_lock_file.txt");

    // Spawn moon test in background
    let mut moon_child = std::process::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["test", "--target", "js", "--no-parallelize"])
        .spawn()
        .expect("Failed to spawn moon test");

    // Wait for lock file to be created
    let start = Instant::now();
    while !lock_file.exists() {
        thread::sleep(Duration::from_millis(100));
        if start.elapsed() > Duration::from_secs(10) {
            panic!("Timeout waiting for lock file to be created");
        }
    }

    // Record the initial value written into the lock file.
    // The MoonBit test writes an incrementing counter into the
    // file on each heartbeat, so we can use this value to detect
    // whether the writer is still alive after we kill `moon`.
    let initial_counter =
        read_lock_counter(&lock_file).expect("Failed to read initial counter from lock file");

    // Terminate the moon process (simulating the scenario in example/script.js)
    terminate_child(&mut moon_child);

    // When moon is killed, all child processes (moonrun/node) should also be terminated.
    // This is verified by checking that the lock file eventually stops
    // being updated after moon is killed.
    // If the file is still being updated until timeout, it means the
    // child process continues running as a zombie.
    // If this assertion fails, it indicates a regression where child
    // processes survive after termination.
    let quiescent = wait_for_lock_quiescent(&lock_file, initial_counter);
    if !quiescent {
        // Clean up moon child process (if still alive)
        moon_child.kill().expect("Failed to terminate moon process");
        moon_child.wait().expect("Failed to wait moon process");
        panic!(
            "Child processes (moonrun/node) are not terminated when moon is killed. \
        The lock file continues to be updated until timeout, indicating that spawned test processes remain running as zombies. \
        Moon should properly propagate termination signals to all child processes."
        );
    }
}

#[cfg(unix)]
fn terminate_child(child: &mut std::process::Child) {
    let pid = child.id() as i32;
    let rc = unsafe { libc::kill(pid, libc::SIGTERM) };
    if rc != 0 {
        panic!(
            "Failed to send SIGTERM to moon process: {}",
            std::io::Error::last_os_error()
        );
    }
}

#[cfg(windows)]
fn terminate_child(child: &mut std::process::Child) {
    child.kill().expect("Failed to terminate moon process");
}

/// Read the current heartbeat counter from the lock file.
///
/// The MoonBit test writes an incrementing integer to the lock file
/// on each heartbeat. This helper reads the file as UTF-8 text,
/// trims it, and parses it as `u64`.
fn read_lock_counter(lock_file: &std::path::Path) -> std::io::Result<u64> {
    let content = std::fs::read_to_string(lock_file)?;
    let trimmed = content.trim();
    let counter = trimmed
        .parse::<u64>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{e}")))?;
    Ok(counter)
}

/// Wait until the lock file's heartbeat counter stops increasing after
/// `last_counter`.
///
/// Returns `true` if, within a bounded time window, we observe the
/// counter become *quiescent* (no longer strictly greater than
/// `last_counter`). This includes the cases where the file disappears
/// or its contents become unreadable, because those all indicate that
/// no further heartbeats are happening from the test process tree.
///
/// Returns `false` only if, for the entire timeout window, every poll
/// observes a strictly larger counter than before, meaning the file
/// keeps getting updated and the child process is likely still running.
fn wait_for_lock_quiescent(lock_file: &std::path::Path, mut last_counter: u64) -> bool {
    use std::time::{Duration, Instant};

    let start = Instant::now();
    let timeout = Duration::from_secs(5);
    loop {
        std::thread::sleep(Duration::from_millis(200));
        if start.elapsed() > timeout {
            return false;
        }

        let counter = match read_lock_counter(lock_file) {
            Ok(counter) => counter,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // File disappeared: treat as quiescent (writer gone).
                return true;
            }
            Err(_) => {
                // Transient or format error: ignore this sample and
                // try again until we either observe quiescence or
                // hit the overall timeout.
                continue;
            }
        };

        if counter <= last_counter {
            return true;
        }
        last_counter = counter;
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
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["info", "--frozen"])
        .assert()
        .success();
    // Check directory structure by listing all files
    let root_dir = dir.as_ref().to_owned();
    let dir = WalkDir::new(&dir)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|e| e.path().strip_prefix(&root_dir).unwrap().to_owned())
        // Filter out target directory
        .filter(|p| !p.starts_with("target"))
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
            _build/wasm-gc/release
            _build/wasm-gc/release/build
            _build/wasm-gc/release/build/.mooncakes
            _build/wasm-gc/release/build/.mooncakes/lijunchen
            _build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake
            _build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake/lib
            _build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake/lib/lib.core
            _build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake/lib/lib.mi
            _build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake/mooncake.core
            _build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake/mooncake.mi
            _build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake2
            _build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake2/lib
            _build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake2/lib/lib.core
            _build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake2/lib/lib.mi
            _build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake2/mooncake2.core
            _build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake2/mooncake2.mi
            _build/wasm-gc/release/build/all_pkgs.json
            _build/wasm-gc/release/build/build.moon_db
            _build/wasm-gc/release/build/lib
            _build/wasm-gc/release/build/lib/lib.core
            _build/wasm-gc/release/build/lib/lib.mi
            _build/wasm-gc/release/build/main
            _build/wasm-gc/release/build/main/main.core
            _build/wasm-gc/release/build/main/main.mi
            _build/wasm-gc/release/build/main/main.wasm
            _build/wasm-gc/release/check
            _build/wasm-gc/release/check/.mooncakes
            _build/wasm-gc/release/check/.mooncakes/lijunchen
            _build/wasm-gc/release/check/.mooncakes/lijunchen/mooncake
            _build/wasm-gc/release/check/.mooncakes/lijunchen/mooncake/lib
            _build/wasm-gc/release/check/.mooncakes/lijunchen/mooncake/lib/lib.mi
            _build/wasm-gc/release/check/.mooncakes/lijunchen/mooncake/mooncake.mi
            _build/wasm-gc/release/check/.mooncakes/lijunchen/mooncake2
            _build/wasm-gc/release/check/.mooncakes/lijunchen/mooncake2/lib
            _build/wasm-gc/release/check/.mooncakes/lijunchen/mooncake2/lib/lib.mi
            _build/wasm-gc/release/check/.mooncakes/lijunchen/mooncake2/mooncake2.mi
            _build/wasm-gc/release/check/all_pkgs.json
            _build/wasm-gc/release/check/check.moon_db
            _build/wasm-gc/release/check/lib
            _build/wasm-gc/release/check/lib/lib.mbti
            _build/wasm-gc/release/check/lib/lib.mi
            _build/wasm-gc/release/check/main
            _build/wasm-gc/release/check/main/main.mbti
            _build/wasm-gc/release/check/main/main.mi
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
            moonc build-package ./mods/lijunchen/mooncake2/src/lib/hello.mbt -w -a -o ./_build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake2/lib/lib.core -pkg lijunchen/mooncake2/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources lijunchen/mooncake2/lib:./mods/lijunchen/mooncake2/src/lib -target wasm-gc -workspace-path ./mods/lijunchen/mooncake2 -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./mods/lijunchen/mooncake2/src/top.mbt -w -a -o ./_build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake2/mooncake2.core -pkg lijunchen/mooncake2 -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake2/lib/lib.mi:lib -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources lijunchen/mooncake2:./mods/lijunchen/mooncake2/src -target wasm-gc -workspace-path ./mods/lijunchen/mooncake2 -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./mods/lijunchen/mooncake/lib/hello.mbt -w -a -o ./_build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake/lib/lib.core -pkg lijunchen/mooncake/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources lijunchen/mooncake/lib:./mods/lijunchen/mooncake/lib -target wasm-gc -workspace-path ./mods/lijunchen/mooncake -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./mods/lijunchen/mooncake/top.mbt -w -a -o ./_build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake/mooncake.core -pkg lijunchen/mooncake -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake/lib/lib.mi:lib -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources lijunchen/mooncake:./mods/lijunchen/mooncake -target wasm-gc -workspace-path ./mods/lijunchen/mooncake -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/release/build/lib/lib.core -pkg hello31/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources hello31/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/release/build/main/main.core -pkg hello31/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/build/lib/lib.mi:lib -i ./_build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake/mooncake.mi:mooncake -i ./_build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake2/mooncake2.mi:mooncake2 -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources hello31/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/release/build/lib/lib.core ./_build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake/lib/lib.core ./_build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake/mooncake.core ./_build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake2/lib/lib.core ./_build/wasm-gc/release/build/.mooncakes/lijunchen/mooncake2/mooncake2.core ./_build/wasm-gc/release/build/main/main.core -main hello31/main -o ./_build/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello31/lib:./lib -pkg-sources lijunchen/mooncake/lib:./mods/lijunchen/mooncake/lib -pkg-sources lijunchen/mooncake:./mods/lijunchen/mooncake -pkg-sources lijunchen/mooncake2/lib:./mods/lijunchen/mooncake2/src/lib -pkg-sources lijunchen/mooncake2:./mods/lijunchen/mooncake2/src -pkg-sources hello31/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc
        "#]],
    );
    assert!(out.contains("lijunchen/mooncake:./mods/lijunchen/mooncake"));
    assert!(out.contains("lijunchen/mooncake2:./mods/lijunchen/mooncake2/src"));
}
#[test]
fn test_moon_test_no_entry_warning() {
    let dir = TestDir::new("moon_test/no_entry_warning");

    let out = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
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
        dir.join("target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt");
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

    let out1 = get_stdout(&dir, ["test", "-C", "async_test_inline"]);
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
            "test",
            "-C",
            "async_test",
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
            "test",
            "-C",
            "async_test",
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
fn test_max_concurrent_tests() {
    let dir = TestDir::new("moon_test");
    let out1 = get_stdout(
        &dir,
        [
            "test",
            "-C",
            "max_concurrent_tests",
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
            "test",
            "-C",
            "max_concurrent_tests",
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
