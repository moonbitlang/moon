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
use std::process::{Command, Stdio};

use crate::util::toolchain_root_for_tests;

use super::TestDir;

const CHILD_OUTPUT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Exercise the fix: install v1, keep a v1 child *running*, then
/// install v2 on top of it. `macos_install_file`'s atomic rename
/// must leave the running v1 alone while the destination path becomes
/// a launchable v2. A pre-fix `fs::copy` reinstall poisons macOS's
/// code-signing cache, so the fresh v2 launch below is killed.
#[test]
fn test_install_replaces_while_running() {
    let moon_exe = PathBuf::from(env!("CARGO_BIN_EXE_moon"));
    let fixture = TestDir::new("install_atomic_rename.in");
    let pkg_path = fixture.join("src/victim");
    let main_mbt = pkg_path.join("main.mbt");
    let install_dir = tempfile::tempdir().expect("install tempdir");
    let victim_path = install_dir.path().join("victim");

    rewrite_version(&main_mbt, "version 1");
    moon_install(&moon_exe, fixture.as_ref(), &pkg_path, install_dir.path());

    let mut v1_child = Command::new(&victim_path)
        .arg("hold")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn v1");
    let v1_stdout = v1_child.stdout.take().expect("capture v1 stdout");
    let v1_lines = child_stdout_lines(v1_stdout);
    wait_for_line(&v1_lines, "version 1", CHILD_OUTPUT_TIMEOUT);

    rewrite_version(&main_mbt, "version 2");
    moon_install(&moon_exe, fixture.as_ref(), &pkg_path, install_dir.path());

    let v2_out = Command::new(&victim_path).output().expect("run v2");
    assert!(v2_out.status.success(), "v2 exit: {:?}", v2_out.status);
    assert_eq!(
        String::from_utf8_lossy(&v2_out.stdout).trim(),
        "version 2",
        "v2 stdout mismatch: reinstall did not leave v2 bytes on disk"
    );

    // The held v1 process should still be able to self-spawn through
    // the destination path after reinstall. A pre-fix in-place copy
    // kills that post-reinstall launch on macOS.
    wait_for_line(&v1_lines, "version 2", CHILD_OUTPUT_TIMEOUT);
    assert!(
        matches!(v1_child.try_wait(), Ok(None)),
        "running v1 died during reinstall: the atomic-rename path did \
         not leave the running process usable"
    );
    let _ = v1_child.kill();
    let _ = v1_child.wait();
}

fn child_stdout_lines(stdout: std::process::ChildStdout) -> std::sync::mpsc::Receiver<String> {
    use std::io::BufRead;

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

fn wait_for_line(
    lines: &std::sync::mpsc::Receiver<String>,
    expected: &str,
    timeout: std::time::Duration,
) {
    let deadline = std::time::Instant::now() + timeout;
    let mut seen = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            panic!("timed out waiting for `{expected}`; seen output: {seen:?}");
        }
        match lines.recv_timeout(remaining) {
            Ok(line) if line.trim() == expected => return,
            Ok(line) => seen.push(line),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                panic!("timed out waiting for `{expected}`; seen output: {seen:?}");
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                panic!("stdout closed while waiting for `{expected}`; seen output: {seen:?}");
            }
        }
    }
}

fn moon_install(moon_exe: &Path, cwd: &Path, pkg_path: &Path, install_dir: &Path) {
    let mut command = Command::new(moon_exe);
    command
        .env("MOON_TOOLCHAIN_ROOT", toolchain_root_for_tests())
        .current_dir(cwd)
        .args(["install", "--path"])
        .arg(pkg_path)
        .arg("--bin")
        .arg(install_dir);

    let output = command.output().expect("run moon install");
    if !output.status.success() {
        panic!(
            "moon install failed: status={}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

/// Write a `main.mbt` whose rodata diverges per `version`: a
/// toplevel `Array[Int]` of sequential values offset by a
/// version-derived constant, so every slot differs between v1 and
/// v2. Summed at startup with the total folded into a branch the
/// compiler cannot prove is dead, which keeps the array from being
/// DCE'd. This gives v1 and v2 substantially different on-disk bytes
/// across many code-signing pages, so an in-place `fs::copy` tamper
/// during reinstall reliably invalidates the running binary's CS
/// cache.
fn rewrite_version(main_mbt: &Path, version: &str) {
    const N: u32 = 50_000;
    let offset: u32 = version
        .as_bytes()
        .iter()
        .fold(0u32, |acc, &b| acc.wrapping_mul(131).wrapping_add(b as u32));
    let mut body = String::with_capacity(N as usize * 8);
    body.push_str("///|\nlet payload : ReadOnlyArray[Int] = [\n");
    for i in 0..N {
        let v = i.wrapping_add(offset) as i32;
        body.push_str(&format!("{v},"));
        if i % 32 == 31 {
            body.push('\n');
        }
    }
    body.push_str("\n]\n\n");
    let main_fn = format!(
        "///|\nasync fn main {{\n  \
           let mut sum : Int64 = 0L\n  \
           for v in payload {{ sum = sum + v.to_int64() }}\n  \
           if sum == 0x7fffffffffffffffL {{ println(\"unreachable\") }}\n  \
           println(\"{version}\")\n  \
           if @env.args().length() > 1 {{\n    \
             let self_path = @env.args()[0]\n    \
             for ;; {{\n      \
               @async.sleep(100)\n      \
               let _ = (@process.run(self_path, []) : Int)\n    \
             }}\n  \
           }}\n\
         }}\n"
    );
    std::fs::write(main_mbt, format!("{body}{main_fn}")).expect("rewrite main.mbt");
}
