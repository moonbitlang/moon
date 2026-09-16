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

fn moon(root: &Path) -> snapbox::cmd::Command {
    snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
        .current_dir(root)
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .env("MOON_BUILD_CACHE", root.join(".cache"))
        .env("MOON_DEP_CACHE", "off")
        .env("MOON_HASH_ENGINE", "1")
}

fn check(root: &Path) -> snapbox::cmd::Command {
    moon(root).args(["check", "--target", "wasm-gc", "--json"])
}

#[test]
fn reuses_outputs_and_replays_warnings_by_content() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("moon.mod.json"), r#"{"name":"test/hash"}"#).unwrap();
    fs::write(root.join("moon.pkg.json"), "{}").unwrap();
    let source = root.join("hello.mbt");
    let text = "pub fn answer() -> Int { let unused = 1; 42 }\n";
    fs::write(&source, text).unwrap();

    check(root).assert().success().stdout_eq(
        r#"{"version":1,"status":"success","diagnostics":[{[..]}],"messages":[],"summary":{"tasks_executed":2,"moon_errors":0,"moon_warnings":0,"diagnostic_errors":0,"diagnostic_warnings":1}}
"#,
    );
    assert!(!root.join("_build/.moon_db").exists());
    check(root).assert().success().stdout_eq(
        r#"{"version":1,"status":"success","diagnostics":[{[..]}],"messages":[],"summary":{"tasks_executed":0,"moon_errors":0,"moon_warnings":0,"diagnostic_errors":0,"diagnostic_warnings":1}}
"#,
    );

    // Rewriting identical bytes changes the timestamp, not the action identity.
    fs::write(&source, text).unwrap();
    check(root).assert().success().stdout_eq(
        r#"[..]"tasks_executed":0,[..]"diagnostic_warnings":1}}
"#,
    );
    let modified = fs::metadata(&source).unwrap().modified().unwrap();
    fs::write(&source, text.replace("42", "43")).unwrap();
    fs::File::options()
        .write(true)
        .open(&source)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    check(root).assert().success().stdout_eq(
        r#"[..]"tasks_executed":2,[..]"diagnostic_warnings":1}}
"#,
    );

    fs::remove_dir_all(root.join("_build")).unwrap();
    check(root).assert().success().stdout_eq(
        r#"[..]"tasks_executed":0,[..]"diagnostic_warnings":1}}
"#,
    );
    assert!(root.join("_build/wasm-gc/debug/check/hash.mi").exists());
}

#[test]
fn builds_and_runs_both_wasm_backends() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("moon.mod.json"), r#"{"name":"test/hash"}"#).unwrap();
    fs::create_dir(root.join("lib")).unwrap();
    fs::write(root.join("lib/moon.pkg.json"), "{}").unwrap();
    fs::write(root.join("lib/lib.mbt"), "pub fn answer() -> Int { 42 }\n").unwrap();
    fs::create_dir(root.join("main")).unwrap();
    fs::write(
        root.join("main/moon.pkg.json"),
        r#"{"is-main":true,"import":["test/hash/lib"]}"#,
    )
    .unwrap();
    fs::write(
        root.join("main/main.mbt"),
        "fn main { println(@lib.answer()) }\ntest { assert_eq(@lib.answer(), 42) }\n",
    )
    .unwrap();
    for backend in ["wasm", "wasm-gc"] {
        moon(root)
            .args(["build", "--target", backend])
            .assert()
            .success();
        moon(root)
            .args(["build", "--target", backend])
            .assert()
            .success()
            .stderr_eq("Finished. moon: no work to do\n");
        moon(root)
            .args(["run", "main", "--target", backend])
            .assert()
            .success()
            .stdout_eq("42\n");
        moon(root)
            .args(["test", "--target", backend])
            .assert()
            .success();
        moon(root)
            .args(["test", "--target", backend])
            .assert()
            .success();
        fs::remove_dir_all(root.join("_build").join(backend)).unwrap();
        moon(root)
            .args(["build", "--target", backend])
            .assert()
            .success()
            .stderr_eq("Finished. moon: no work to do\n");
        moon(root)
            .args(["run", "main", "--target", backend])
            .assert()
            .success()
            .stdout_eq("42\n");
    }
    assert!(!root.join("_build/.moon_db").exists());
}

#[test]
fn failed_compilations_replay_diagnostics_without_publishing_outputs() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("moon.mod.json"), r#"{"name":"test/hash"}"#).unwrap();
    fs::write(root.join("moon.pkg.json"), "{}").unwrap();
    fs::write(root.join("hello.mbt"), "pub fn answer() -> Int { false }\n").unwrap();
    for invocation in 0..2 {
        check(root).assert().failure().stdout_eq(
            r#"{"version":1,"status":"failure","diagnostics":[{[..]}],"messages":[],"summary":{"tasks_executed":null,"moon_errors":0,"moon_warnings":0,"diagnostic_errors":1,"diagnostic_warnings":0}}
"#);
        let records = fs::read_dir(root.join(".cache/v1"))
            .unwrap()
            .flat_map(|shard| fs::read_dir(shard.unwrap().path()).unwrap())
            .map(|entry| entry.unwrap().path().join("result/record.json"))
            .filter(|record| record.exists())
            .collect::<Vec<_>>();
        // The failed source action blocks its dependent blackbox check.
        assert_eq!(records.len(), 1);
        let record = &records[0];
        let result: serde_json::Value = serde_json::from_slice(&fs::read(record).unwrap()).unwrap();
        assert_ne!(result["exit_code"], 0);
        assert_eq!(result["outputs"], serde_json::json!([]));
        assert!(!record.parent().unwrap().join("0").exists());
        if invocation == 0 {
            fs::File::options()
                .write(true)
                .open(record)
                .unwrap()
                .set_modified(std::time::SystemTime::UNIX_EPOCH)
                .unwrap();
        } else {
            // A rerun would republish the record. A hit leaves it untouched.
            assert_eq!(
                fs::metadata(record).unwrap().modified().unwrap(),
                std::time::SystemTime::UNIX_EPOCH
            );
        }
    }
    fs::write(root.join("hello.mbt"), "pub fn answer() -> Int { 42 }\n").unwrap();
    check(root).assert().success();
    check(root)
        .assert()
        .success()
        .stdout_eq("[..]\"tasks_executed\":0,[..]\n");
}

#[test]
fn corruption_is_a_miss_and_environment_changes_invalidate() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("moon.mod.json"), r#"{"name":"test/hash"}"#).unwrap();
    fs::write(root.join("moon.pkg.json"), "{}").unwrap();
    fs::write(root.join("hello.mbt"), "pub fn answer() -> Int { 42 }\n").unwrap();
    check(root).assert().success();
    for blob in ["0", "record.json", "diagnostics"] {
        for shard in fs::read_dir(root.join(".cache/v1")).unwrap() {
            for entry in fs::read_dir(shard.unwrap().path()).unwrap() {
                fs::write(entry.unwrap().path().join("result").join(blob), "corrupt").unwrap();
            }
        }
        check(root)
            .assert()
            .success()
            .stdout_eq("[..]\"tasks_executed\":2,[..]\n");
        check(root)
            .assert()
            .success()
            .stdout_eq("[..]\"tasks_executed\":0,[..]\n");
    }
    check(root)
        .env("MOON_TEST_HASH_ENV", "changed")
        .assert()
        .success()
        .stdout_eq("[..]\"tasks_executed\":2,[..]\n");
    check(root)
        .env("MOON_TEST_HASH_ENV", "changed")
        .assert()
        .success()
        .stdout_eq("[..]\"tasks_executed\":0,[..]\n");
    check(root)
        .env("MOON_BUILD_CACHE", "off")
        .assert()
        .success()
        .stdout_eq("[..]\"tasks_executed\":2,[..]\n");
    check(root)
        .env("MOON_BUILD_CACHE", "off")
        .assert()
        .success()
        .stdout_eq("[..]\"tasks_executed\":2,[..]\n");
    assert!(!root.join("_build/.moon_hash").exists());
}

#[test]
fn a_waiter_reuses_the_result_published_under_the_directory_lock() {
    use std::{
        io::{BufRead, BufReader},
        process::{Command, Stdio},
        sync::mpsc,
        time::Duration,
    };
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("moon.mod.json"), r#"{"name":"test/hash"}"#).unwrap();
    fs::write(root.join("moon.pkg.json"), "{}").unwrap();
    fs::write(root.join("hello.mbt"), "pub fn answer() -> Int { 42 }\n").unwrap();
    moon(root)
        .args(["check", "--target", "wasm-gc"])
        .assert()
        .success();
    let shard = fs::read_dir(root.join(".cache/v1"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let entry = fs::read_dir(shard).unwrap().next().unwrap().unwrap().path();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(entry.join(moonutil::constants::MOON_LOCK))
        .unwrap();
    lock.lock().unwrap();
    fs::rename(entry.join("result"), entry.join("pending")).unwrap();
    let mut child = Command::new(snapbox::cargo_bin!("moon"))
        .current_dir(root)
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .env("MOON_BUILD_CACHE", root.join(".cache"))
        .env("MOON_DEP_CACHE", "off")
        .env("MOON_HASH_ENGINE", "1")
        .args(["check", "--target", "wasm-gc"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let (send, receive) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut output = String::new();
        for line in BufReader::new(stderr).lines() {
            let line = line.unwrap();
            if line.contains("Blocking waiting for file lock") {
                let _ = send.send(());
            }
            output.push_str(&line);
            output.push('\n');
        }
        output
    });
    let waited = receive.recv_timeout(Duration::from_secs(30));
    fs::rename(entry.join("pending"), entry.join("result")).unwrap();
    drop(lock);
    if waited.is_err() {
        let _ = child.kill();
    }
    let output = child.wait_with_output().unwrap();
    let stderr = reader.join().unwrap();
    waited.expect("child must actually wait for the cache directory lock");
    assert!(output.status.success(), "{stderr}");
    snapbox::assert_data_eq!(
        stderr,
        "Blocking waiting for file lock [..] ...\nFinished. moon: no work to do\n"
    );
}

#[test]
fn prebuild_left() {
    prebuild_helper("left", "right");
}
#[test]
fn prebuild_right() {
    prebuild_helper("right", "left");
}

fn prebuild_helper(side: &str, other: &str) {
    use std::time::{Duration, Instant};
    let Ok(mode) = std::env::var("MOON_TEST_HASH_PREBUILD") else {
        return;
    };
    let marker = format!(".{side}-ready");
    fs::write(marker, "ready").unwrap();
    if mode == "parallel" {
        let started = Instant::now();
        while !Path::new(&format!(".{other}-ready")).exists() {
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "independent prebuild actions must run concurrently"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    } else {
        fs::create_dir(".active-prebuild").expect("--jobs 1 must serialize actions");
        std::thread::sleep(Duration::from_millis(100));
        fs::remove_dir(".active-prebuild").unwrap();
    }
    // Also exercises arbitrary prebuild output, working directory, and stderr
    // capture without making that shell action eligible for caching.
    eprintln!("prebuild {side}");
    fs::write(
        format!("{side}/generated.mbt"),
        "pub fn generated() -> Int { 7 }\n",
    )
    .unwrap();
}

#[test]
fn schedules_dependencies_and_respects_parallelism() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("moon.mod.json"), r#"{"name":"test/hash"}"#).unwrap();
    let executable = std::env::current_exe().unwrap().display().to_string();
    for side in ["left", "right"] {
        fs::create_dir(root.join(side)).unwrap();
        fs::write(root.join(side).join("input.txt"), "input").unwrap();
        let test = format!("prebuild_{side}");
        let command = moonutil::shlex::join_native(
            [executable.as_str(), "--exact", &test, "--nocapture"].into_iter(),
        );
        let manifest = serde_json::json!({"pre-build":[{"input":"input.txt","output":"generated.mbt","command":command}]});
        fs::write(root.join(side).join("moon.pkg.json"), manifest.to_string()).unwrap();
    }
    for (mode, jobs) in [("parallel", "2"), ("serial", "1")] {
        check(root)
            .env("MOON_TEST_HASH_PREBUILD", mode)
            .args(["--jobs", jobs])
            .assert()
            .success();
    }
    for side in ["left", "right"] {
        assert_eq!(
            fs::read_to_string(root.join(side).join("generated.mbt")).unwrap(),
            "pub fn generated() -> Int { 7 }\n"
        );
    }
    // Unmodeled shell inputs make both prebuild actions and their consumers
    // ineligible; every invocation must execute them, even with existing outputs.
    assert_eq!(fs::read_dir(root.join(".cache/v1")).unwrap().count(), 0);
}

#[test]
fn executes_and_reuses_response_file_commands() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("moon.mod.json"), r#"{"name":"test/hash"}"#).unwrap();
    fs::write(root.join("moon.pkg.json"), "{}").unwrap();
    for i in 0..120 {
        fs::write(
            root.join(format!("file_{i}_{}.mbt", "x".repeat(150))),
            format!("pub fn value{i}() -> Int {{ {i} }}\n"),
        )
        .unwrap();
    }
    check(root).assert().success();
    assert!(root.join("_build/wasm-gc/debug/check/hash.mi.rsp").exists());
    check(root)
        .assert()
        .success()
        .stdout_eq("[..]\"tasks_executed\":0,[..]\n");
}

#[test]
fn snapshot_updates_recompute_the_requested_action_closure() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("moon.mod.json"), r#"{"name":"test/hash"}"#).unwrap();
    fs::write(root.join("moon.pkg.json"), "{}").unwrap();
    let source = root.join("hello.mbt");
    fs::write(&source, "test { inspect(1, content=\"0\") }\n").unwrap();
    moon(root)
        .args(["test", "--target", "wasm-gc", "--update"])
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(&source).unwrap(),
        "test { inspect(1, content=(\n  #|1\n)) }\n"
    );
    moon(root)
        .args(["test", "--target", "wasm-gc"])
        .assert()
        .success();
    assert!(!root.join("_build/.moon_db").exists());
}
