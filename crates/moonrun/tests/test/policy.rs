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

use std::{net::Ipv4Addr, path::PathBuf, time::Duration};

use super::{MOONBIT_ASYNC_CHECK_FD_LEAK, moon_bin, moon_cmd};

#[test]
fn test_moon_run_policy_with_workspace_async_fs() {
    let case_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases");
    let case_dir = case_root.join("test_policy_workspace.in");
    let dir = tempfile::Builder::new()
        .prefix("test_policy_workspace.")
        .tempdir_in(&case_root)
        .expect("create temp fixture");
    moon_test_util::test_dir::copy_tree(&case_dir, dir.path(), false).expect("copy test fixture");

    moon_cmd()
        .current_dir(dir.path())
        .args(["build", "--target", "wasm"])
        .assert()
        .success();
    let wasm_file = |package: &str| {
        std::fs::canonicalize(dir.path().join(format!(
            "_build/wasm/debug/build/moon/policy_workspace/{package}/{package}.wasm"
        )))
        .unwrap()
    };
    let fs_allow_wasm = wasm_file("fs_allow");
    let fs_deny_read_wasm = wasm_file("fs_deny_read");
    let env_get_wasm = wasm_file("env_get");
    // Moonx runs cached artifacts as ordinary programs. This probe cannot be a
    // test executable because a MoonBit test driver does nothing without
    // moonrun's --test-args selection.
    let env_marker_wasm = wasm_file("env_marker");
    let detach_moonx_wasm = wasm_file("detach_moonx");
    let env_mutate_wasm = wasm_file("env_mutate");
    let listen_implicit_bind_wasm = wasm_file("listen_implicit_bind");
    let process_env_wasm = wasm_file("process_env");
    let policy_file = std::fs::canonicalize(dir.path().join("policy.toml")).unwrap();
    let deny_all_policy_file = std::fs::canonicalize(dir.path().join("deny-all.toml")).unwrap();
    let process_any_args_policy_file =
        std::fs::canonicalize(dir.path().join("process-any-args.toml")).unwrap();
    let process_prefix_deny_policy_file =
        std::fs::canonicalize(dir.path().join("process-prefix-deny.toml")).unwrap();

    #[cfg(not(windows))]
    let fs_deny_read_stderr = snapbox::str![[r#"
Sandbox policy blocked file read: "denied/secret.txt"
OSError("[..]@fs.open()[..]denied/secret.txt[..]Permission denied")

"#]];
    #[cfg(windows)]
    let fs_deny_read_stderr = snapbox::str![[r#"
Sandbox policy blocked file read: "denied/secret.txt"
OSError("[..]@fs.open()[..]denied/secret.txt[..]Access is denied.")

"#]];
    #[cfg(not(windows))]
    let env_mutate_stdout = "runtime override\nmissing\n/first/\n/second/\n/tmp/\n";
    #[cfg(windows)]
    let env_mutate_stdout = "runtime override\nmissing\nC:/First\\\nC:/Second\\\nmissing\n";

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .arg("--policy")
        .arg(&policy_file)
        .arg(&fs_allow_wasm)
        .assert()
        .success()
        .stdout_eq("workspace input\n|workspace sandbox policy\n");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .arg("--policy")
        .arg(&policy_file)
        .arg(&env_get_wasm)
        .assert()
        .success()
        .stdout_eq("configured by policy\n");

    // Policy construction keeps its canonical inheritance payload in memory.
    // A Run that never spawns moonx therefore works even when the platform temp
    // directory is itself the Runtime Working Directory.
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(std::env::temp_dir())
        .arg("--policy")
        .arg(&policy_file)
        .arg(&env_get_wasm)
        .assert()
        .success()
        .stdout_eq("configured by policy\n");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .arg("--policy")
        .arg(&policy_file)
        .arg(&env_mutate_wasm)
        .assert()
        .success()
        .stdout_eq(snapbox::Data::text(env_mutate_stdout).raw());

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .env("MOONRUN_HIDDEN_ENV", "host secret")
        .arg("--policy")
        .arg(&policy_file)
        .arg(&process_env_wasm)
        .assert()
        .success()
        .stdout_eq(if cfg!(windows) {
            "MOONRUN_POLICY_ENV=configured by policy\n"
        } else {
            "configured by policy|\n"
        });

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .env("MOONRUN_HIDDEN_ENV", "host secret")
        .arg("--policy")
        .arg(&process_any_args_policy_file)
        .arg(&process_env_wasm)
        .assert()
        .success()
        .stdout_eq(if cfg!(windows) {
            "MOONRUN_POLICY_ENV=configured by policy\n"
        } else {
            "configured by policy|\n"
        });

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .env("MOONRUN_POLICY_ENV", "host value")
        .arg("--policy")
        .arg(&deny_all_policy_file)
        .arg(&env_get_wasm)
        .assert()
        .success()
        .stdout_eq("missing\n");

    let moon_home = tempfile::TempDir::new().expect("create inherited-policy MOON_HOME");
    let cached_wasm = moon_home
        .path()
        .join("registry/cache/assets/moonbitlang/parser/0.3.3/cmd/moonfmt/moonfmt.wasm");
    std::fs::create_dir_all(cached_wasm.parent().unwrap()).unwrap();
    std::fs::copy(&env_marker_wasm, &cached_wasm).unwrap();

    let bin_dir = tempfile::TempDir::new().expect("create inherited-policy bin directory");
    let moonx = bin_dir
        .path()
        .join(if cfg!(windows) { "moonx.exe" } else { "moonx" });
    std::fs::hard_link(moon_bin(), &moonx).unwrap_or_else(|_| {
        std::fs::copy(moon_bin(), &moonx).expect("copy moon binary as moonx");
    });
    let path = std::env::join_paths(std::iter::once(bin_dir.path().to_path_buf()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .expect("put moonx on PATH");
    let inherited_policy = dir.path().join("inherited-policy.toml");
    std::fs::write(
        &inherited_policy,
        r#"
[env]
from_host = ["*"]

[env.set]
MOONRUN_INHERITED_POLICY = "guest"
MOONRUN_POLICY_ENV = "inherited"

[fs]
write = ["*"]

[process]
spawn = true
"#,
    )
    .unwrap();

    // The blackbox test removes this directory through the guest filesystem,
    // then verifies that the following direct moonx spawn reports the policy-
    // transfer failure instead of creating a child.
    let policy_transfer_temp = tempfile::TempDir::new().expect("create policy transfer temp");
    moon_cmd()
        .current_dir(dir.path())
        .env("PATH", &path)
        .env("MOON_HOME", moon_home.path())
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .env("TMPDIR", policy_transfer_temp.path())
        .env("TMP", policy_transfer_temp.path())
        .env("TEMP", policy_transfer_temp.path())
        .args([
            "test",
            "--package",
            "moon/policy_workspace/spawn_moonx_error",
            "--target",
            "wasm",
            "--wasm-policy",
        ])
        .arg(&inherited_policy)
        .assert()
        .success()
        .stdout_eq("Total tests: 1, passed: 1, failed: 0.\n");

    moon_cmd()
        .current_dir(dir.path())
        .env("PATH", &path)
        .env("MOON_HOME", moon_home.path())
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .args([
            "test",
            "--package",
            "moon/policy_workspace/spawn_moonx",
            "--target",
            "wasm",
            "--wasm-policy",
        ])
        .arg(&inherited_policy)
        .assert()
        .success()
        .stdout_eq("Total tests: 1, passed: 1, failed: 0.\n");

    #[cfg(any(unix, windows))]
    {
        // Each moonx-named wrapper announces itself over TCP and waits for the
        // test to release it. The test does not send that release until the
        // parent moonrun has exited, so the detached lifetime boundary is an
        // event in the protocol rather than a filesystem timing assumption.
        let wrapper_dir =
            tempfile::TempDir::new().expect("create detached moonx wrapper directory");
        let wrapper = wrapper_dir
            .path()
            .join(format!("moonx{}", std::env::consts::EXE_SUFFIX));
        let wrapper_source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/detached_moonx_wrapper.rs");
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let status = std::process::Command::new(rustc)
            .arg("--edition=2021")
            .arg(wrapper_source)
            .arg("-o")
            .arg(&wrapper)
            .status()
            .expect("compile detached moonx wrapper");
        assert!(status.success(), "failed to compile detached moonx wrapper");

        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .expect("bind detached moonx synchronization listener");
        listener
            .set_nonblocking(true)
            .expect("make detached moonx listener nonblocking");
        let server = listener
            .local_addr()
            .expect("read detached moonx listener address")
            .to_string();
        let detached_path = std::env::join_paths(
            std::iter::once(wrapper_dir.path().to_path_buf()).chain(std::env::split_paths(&path)),
        )
        .expect("put detached moonx wrapper on PATH");
        let log = dir.path().join("detached-moonx");

        snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
            .current_dir(dir.path())
            .env("PATH", detached_path)
            .env("MOON_HOME", moon_home.path())
            .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
            .env("MOONRUN_OVERRIDE", snapbox::cmd::cargo_bin!("moonrun"))
            .env("MOON_TEST_REAL_MOONX", &moonx)
            .env("MOON_TEST_DETACH_SERVER", server)
            .env("MOON_TEST_DETACH_LOG", &log)
            .arg("--policy")
            .arg(&inherited_policy)
            .arg(&detach_moonx_wasm)
            .assert()
            .success()
            .stdout_eq("")
            .stderr_eq("");

        let logs = || {
            (0..2)
                .map(|index| {
                    let path = log.with_file_name(format!("detached-moonx-{index}.log"));
                    let contents = std::fs::read_to_string(&path)
                        .unwrap_or_else(|error| format!("<failed to read: {error}>"));
                    format!("{}:\n{contents}", path.display())
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        // Time only bounds a broken test. Every successful transition below
        // is driven by a byte received from the process on the other side.
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        let mut children = Vec::with_capacity(2);
        while children.len() < 2 {
            match listener.accept() {
                Ok((stream, _)) => children.push(stream),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() >= deadline {
                        panic!("detached moonx did not connect before timeout\n{}", logs());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("failed to accept detached moonx: {error}\n{}", logs()),
            }
        }

        for child in &mut children {
            child
                .set_nonblocking(false)
                .expect("make detached moonx connection blocking");
            child
                .set_read_timeout(Some(Duration::from_secs(60)))
                .expect("set detached moonx read timeout");
            child
                .set_write_timeout(Some(Duration::from_secs(60)))
                .expect("set detached moonx write timeout");
            let mut ready = [0];
            std::io::Read::read_exact(child, &mut ready).unwrap_or_else(|error| {
                panic!("detached moonx did not become ready: {error}\n{}", logs())
            });
            assert_eq!(ready, [b'R'], "unexpected detached moonx handshake");
        }

        for child in &mut children {
            std::io::Write::write_all(child, b"G").unwrap_or_else(|error| {
                panic!("failed to release detached moonx: {error}\n{}", logs())
            });
        }
        for child in &mut children {
            let mut result = [0];
            std::io::Read::read_exact(child, &mut result).unwrap_or_else(|error| {
                panic!(
                    "detached moonx did not report completion: {error}\n{}",
                    logs()
                )
            });
            assert_eq!(
                result,
                [b'S'],
                "detached moonx failed after its parent exited\n{}",
                logs()
            );
        }
    }

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .arg("--policy")
        .arg(&process_prefix_deny_policy_file)
        .arg(&process_env_wasm)
        .assert()
        .success()
        .stdout_eq("spawn denied\n")
        .stderr_eq("Sandbox policy blocked process spawn\n");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .arg("--policy")
        .arg(&deny_all_policy_file)
        .arg(&listen_implicit_bind_wasm)
        .assert()
        .success()
        .stdout_eq("listen denied\n")
        .stderr_eq(snapbox::str![[r#"
Sandbox policy blocked network bind: "0.0.0.0:0"

"#]]);

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .arg("--policy")
        .arg(&deny_all_policy_file)
        .arg(&process_env_wasm)
        .assert()
        .success()
        .stdout_eq("spawn denied\n")
        .stderr_eq("Sandbox policy blocked process spawn\n");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .arg("--policy")
        .arg(&policy_file)
        .arg(&fs_deny_read_wasm)
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq(fs_deny_read_stderr);
}
