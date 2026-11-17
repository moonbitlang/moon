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

use std::path::PathBuf;

use super::{MOONBIT_ASYNC_CHECK_FD_LEAK, TestDir, moon_cmd};

#[test]
fn test_moonrun_current_exe_absolutizes_relative_module_path() {
    let dir = TestDir::new("test_current_exe.in");
    // Resolve Unix cwd symlinks, but do not introduce a Windows verbatim prefix.
    let load_dir = dunce::canonicalize(&dir).unwrap();
    let built_module = load_dir.join("_build/wasm/debug/test/main/main.blackbox_test.wasm");

    moon_cmd()
        .current_dir(&load_dir)
        .args(["test", "main", "--target", "wasm", "--build-only"])
        .assert()
        .success();

    let module_paths = [
        PathBuf::from("executable/main.wasm"),
        PathBuf::from("路径-\u{10000}/程序.wasm"),
        // Filesystems such as APFS reject these names; exercise them on Linux.
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::ffi::OsStringExt;
            PathBuf::from(std::ffi::OsString::from_vec(
                b"non-utf8-\xff/module-\xfe.wasm".to_vec(),
            ))
        },
    ];
    for relative_path in module_paths {
        let module_path = std::path::absolute(load_dir.join(&relative_path)).unwrap();
        std::fs::create_dir_all(module_path.parent().unwrap()).unwrap();
        std::fs::copy(&built_module, &module_path).unwrap();

        snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
            .current_dir(&load_dir)
            .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
            .env(
                "MOONRUN_TEST_CURRENT_EXE",
                module_path.to_string_lossy().as_ref(),
            )
            .env(
                "MOONRUN_TEST_CURRENT_EXE_DIR",
                module_path.parent().unwrap().to_string_lossy().as_ref(),
            )
            .arg("--test-args")
            .arg("main_test.mbt:0-1")
            .arg(&relative_path)
            .assert()
            .success()
            .stdout_eq(snapbox::str![[r#"
----- BEGIN MOON TEST RESULT -----
{"type":"start","file":"main_test.mbt","index":0}
----- END MOON TEST RESULT -----
----- BEGIN MOON TEST RESULT -----
{"type":"result","file":"main_test.mbt","index":0,"message":""}
----- END MOON TEST RESULT -----

"#]])
            .stderr_eq("");
    }
}

#[test]
fn moonrun_observes_each_process_working_directory() {
    let case_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases");
    let case_dir = case_root.join("test_working_directory_instances.in");
    let dir = tempfile::Builder::new()
        .prefix("test_working_directory_instances.")
        .tempdir_in(&case_root)
        .expect("create temp fixture");
    moon_test_util::test_dir::copy_tree(&case_dir, dir.path(), false).expect("copy test fixture");

    moon_cmd()
        .current_dir(dir.path())
        .args(["test", "main", "--target", "wasm", "--build-only"])
        .assert()
        .success();

    let wasm = dir
        .path()
        .join("_build/wasm/debug/test/main/main.blackbox_test.wasm");
    for marker in ["first", "second"] {
        let instance = dir.path().join(marker);
        std::fs::create_dir(&instance).unwrap();
        std::fs::write(instance.join("input.txt"), marker).unwrap();

        snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
            .current_dir(&instance)
            .arg("--test-args")
            .arg("main_test.mbt:0-1")
            .arg(&wasm)
            .assert()
            .success()
            .stdout_eq(snapbox::str![[r#"
----- BEGIN MOON TEST RESULT -----
{"type":"start","file":"main_test.mbt","index":0}
----- END MOON TEST RESULT -----
----- BEGIN MOON TEST RESULT -----
{"type":"result","file":"main_test.mbt","index":0,"message":""}
----- END MOON TEST RESULT -----

"#]])
            .stderr_eq("");

        assert_eq!(
            std::fs::read_to_string(instance.join("output.txt")).unwrap(),
            marker
        );
        assert_eq!(
            std::fs::canonicalize(PathBuf::from(
                std::fs::read_to_string(instance.join("cwd.txt")).unwrap(),
            ))
            .unwrap(),
            std::fs::canonicalize(instance).unwrap(),
        );
    }
}

#[test]
fn test_moon_run_with_is_windows() {
    let dir = TestDir::new("test_os_platform_detection");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm-gc"])
        .assert()
        .success();

    let wasm_file = dir.join("_build/wasm-gc/debug/build/main/main.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .assert()
        .success()
        .stdout_eq(if std::env::consts::OS == "windows" {
            "1\n"
        } else {
            "0\n"
        });
}

#[test]
fn moonrun_ambient_path_lookup_observes_guest_environment_mutation() {
    let case_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases");
    let case_dir = case_root.join("test_ambient_process_path.in");
    let dir = tempfile::Builder::new()
        .prefix("test_ambient_process_path.")
        .tempdir_in(&case_root)
        .expect("create temp fixture");
    moon_test_util::test_dir::copy_tree(&case_dir, dir.path(), false).expect("copy test fixture");

    let ambient_path_bin = dir.path().join("ambient-path-bin");
    std::fs::create_dir(&ambient_path_bin).expect("create ambient PATH directory");
    std::fs::copy(
        snapbox::cmd::cargo_bin!("moonrun"),
        ambient_path_bin.join(if cfg!(windows) {
            "moonrun-path-command.exe"
        } else {
            "moonrun-path-command"
        }),
    )
    .expect("copy PATH lookup command");

    // The command exists only on the PATH written by the guest test.
    moon_cmd()
        .current_dir(dir.path())
        .env("MOONRUN_TEST_PATH", ambient_path_bin)
        .args(["test", "main", "--target", "wasm"])
        .assert()
        .success()
        .stdout_eq("Total tests: 1, passed: 1, failed: 0.\n");
}
