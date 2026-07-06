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

struct KillOnDrop(std::process::Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Exercise the fix: install v1, keep a v1 child *running*, then
/// install v2 on top of it. Atomic replacement must leave the running
/// v1 alone while the destination path becomes a launchable v2. A
/// pre-fix `fs::copy` reinstall poisons macOS's code-signing cache,
/// so the fresh v2 launch below is killed.
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

    let v1_out = Command::new(&victim_path).output().expect("run v1");
    assert!(v1_out.status.success(), "v1 exit: {:?}", v1_out.status);
    assert_eq!(
        String::from_utf8_lossy(&v1_out.stdout).trim(),
        "version 1",
        "v1 stdout mismatch: initial install did not leave v1 bytes on disk"
    );

    let mut v1_child = KillOnDrop(
        Command::new(&victim_path)
            .arg("hold")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn v1"),
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(
        matches!(v1_child.0.try_wait(), Ok(None)),
        "held v1 exited before reinstall"
    );

    rewrite_version(&main_mbt, "version 2");
    moon_install(&moon_exe, fixture.as_ref(), &pkg_path, install_dir.path());

    let v2_out = Command::new(&victim_path).output().expect("run v2");
    assert!(v2_out.status.success(), "v2 exit: {:?}", v2_out.status);
    assert_eq!(
        String::from_utf8_lossy(&v2_out.stdout).trim(),
        "version 2",
        "v2 stdout mismatch: reinstall did not leave v2 bytes on disk"
    );

    assert!(
        matches!(v1_child.0.try_wait(), Ok(None)),
        "running v1 died during reinstall: the atomic-rename path did \
         not leave the running process usable"
    );
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
        "///|\nfn main {{\n  \
           let mut sum : Int64 = 0L\n  \
           for v in payload {{ sum = sum + v.to_int64() }}\n  \
           if sum == 0x7fffffffffffffffL {{ println(\"unreachable\") }}\n  \
           println(\"{version}\")\n  \
           if @env.args().length() > 1 {{\n    \
             for ;; {{\n      \
             }}\n  \
           }}\n\
         }}\n"
    );
    std::fs::write(main_mbt, format!("{body}{main_fn}")).expect("rewrite main.mbt");
}
