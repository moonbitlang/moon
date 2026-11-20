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

//! `moon upgrade` delegates to an installer script that removes `lib` and
//! `include` before it has a verified replacement (see
//! `crates/moon/src/cli/upgrade/installer.rs`). Any failure after that point
//! used to leave an unusable toolchain with no way back.
//!
//! This drives that exact sequence through a local HTTP server, so no network
//! and no real toolchain are involved: the served script stands in for the
//! installer. The script does need a real interpreter for its platform
//! (`powershell` on Windows, `sh` elsewhere), so every case writes a marker
//! file and fails when the script did not run rather than skipping silently.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;

/// Serves `script` to every connection, so the installer can fetch it from
/// `<url>/install/powershell.ps1` or `<url>/install/unix.sh`.
struct ScriptServer {
    url: String,
}

impl ScriptServer {
    fn start(script: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local script server");
        let addr = listener.local_addr().expect("local addr");
        let body = script.to_string();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut request = [0u8; 2048];
                let _ = stream.read(&mut request);
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(body.as_bytes());
                let _ = stream.flush();
            }
        });
        Self {
            url: format!("http://{addr}"),
        }
    }
}

/// The minimum layout `moon` needs to consider an installation usable.
fn write_previous_toolchain(home: &Path) {
    let files = [
        (
            format!("bin/moonc{}", std::env::consts::EXE_SUFFIX),
            "stub".to_string(),
        ),
        ("lib/moonbitrun.o".to_string(), "stub".to_string()),
        (
            "lib/core/moon.mod".to_string(),
            "name = \"moonbitlang/core\"\n".to_string(),
        ),
        (
            "lib/core/builtin/builtin.mbt".to_string(),
            "pub fn stub() -> Unit {}\n".to_string(),
        ),
        (
            "lib/core/previous-install".to_string(),
            "only present in the tree from before the upgrade".to_string(),
        ),
        ("include/moonbit.h".to_string(), "/* stub */\n".to_string()),
    ];
    for (relative, contents) in files {
        let path = home.join(&relative);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create toolchain dirs");
        std::fs::write(&path, contents).expect("write toolchain file");
    }
}

#[test]
fn failed_upgrade_restores_the_previous_toolchain() {
    let home = tempfile::tempdir().expect("toolchain tempdir");
    write_previous_toolchain(home.path());

    // Mirrors the shipped installers: remove the old tree, then fail before
    // producing a replacement. The marker proves the script actually ran, so a
    // blocked or missing interpreter cannot turn this test into a no-op.
    let script = if cfg!(windows) {
        r#"
if (Test-Path -LiteralPath "$env:MOON_HOME\lib") { Remove-Item -Recurse -Force -LiteralPath "$env:MOON_HOME\lib" }
if (Test-Path -LiteralPath "$env:MOON_HOME\include") { Remove-Item -Recurse -Force -LiteralPath "$env:MOON_HOME\include" }
New-Item -ItemType File -Path "$env:MOON_HOME\installer-ran" -Force | Out-Null
exit 1
"#
    } else {
        r#"
rm -rf "$MOON_HOME/lib" "$MOON_HOME/include"
: > "$MOON_HOME/installer-ran"
exit 1
"#
    };
    let server = ScriptServer::start(script);

    let output = Command::new(env!("CARGO_BIN_EXE_moon"))
        .args([
            "upgrade",
            "--force",
            "--non-interactive",
            "--base-url",
            &server.url,
        ])
        .env("MOON_HOME", home.path())
        .env_remove("MOON_TOOLCHAIN_ROOT")
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .output()
        .expect("run moon upgrade");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report = format!("--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}");

    assert!(
        home.path().join("installer-ran").is_file(),
        "the installer script never ran, so nothing was exercised\n{report}"
    );
    assert!(
        !output.status.success(),
        "a failing installer must fail `moon upgrade`\n{report}"
    );
    assert!(
        home.path().join("lib/core/moon.mod").is_file(),
        "`lib/core` was not restored after the installer failed\n{report}"
    );
    assert!(
        home.path().join("lib/core/previous-install").is_file(),
        "the restored `lib/core` is not the tree from before the upgrade\n{report}"
    );
    assert!(
        home.path().join("include/moonbit.h").is_file(),
        "`include` was not restored after the installer failed\n{report}"
    );
    for backup in ["lib.bak-upgrade", "include.bak-upgrade"] {
        assert!(
            !home.path().join(backup).exists(),
            "`{backup}` should be consumed by the restore\n{report}"
        );
    }
}

#[test]
fn successful_upgrade_keeps_the_new_toolchain() {
    let home = tempfile::tempdir().expect("toolchain tempdir");
    write_previous_toolchain(home.path());

    // A complete replacement: the upgrade must accept it and drop the backup
    // instead of putting the previous tree back. `bin/moonc` is written too —
    // it is the one gate check that the previous tree cannot satisfy by itself.
    let exe = std::env::consts::EXE_SUFFIX;
    let script = if cfg!(windows) {
        format!(
            r#"
New-Item -ItemType Directory -Force -Path "$env:MOON_HOME\lib\core", "$env:MOON_HOME\include", "$env:MOON_HOME\bin" | Out-Null
Set-Content -Path "$env:MOON_HOME\lib\core\moon.mod" -Value 'name = "moonbitlang/core"'
Set-Content -Path "$env:MOON_HOME\include\moonbit.h" -Value '/* new */'
Set-Content -Path "$env:MOON_HOME\bin\moonc{exe}" -Value 'new moonc'
New-Item -ItemType File -Path "$env:MOON_HOME\installer-ran" -Force | Out-Null
exit 0
"#
        )
    } else {
        format!(
            r#"
mkdir -p "$MOON_HOME/lib/core" "$MOON_HOME/include" "$MOON_HOME/bin"
printf 'name = "moonbitlang/core"\n' > "$MOON_HOME/lib/core/moon.mod"
printf '/* new */\n' > "$MOON_HOME/include/moonbit.h"
printf 'new moonc\n' > "$MOON_HOME/bin/moonc{exe}"
: > "$MOON_HOME/installer-ran"
exit 0
"#
        )
    };
    let server = ScriptServer::start(&script);

    let output = Command::new(env!("CARGO_BIN_EXE_moon"))
        .args([
            "upgrade",
            "--force",
            "--non-interactive",
            "--base-url",
            &server.url,
        ])
        .env("MOON_HOME", home.path())
        .env_remove("MOON_TOOLCHAIN_ROOT")
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .output()
        .expect("run moon upgrade");

    let report = format!(
        "--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        home.path().join("installer-ran").is_file(),
        "the installer script never ran, so nothing was exercised\n{report}"
    );
    assert!(
        output.status.success(),
        "a complete replacement must be accepted\n{report}"
    );
    assert!(
        !home.path().join("lib/core/previous-install").exists(),
        "the previous `lib` was put back over a successful install\n{report}"
    );
    let moonc = std::fs::read_to_string(home.path().join(format!("bin/moonc{exe}")))
        .expect("read the replaced moonc");
    assert!(
        moonc.contains("new moonc"),
        "the replacement `bin/moonc` was not kept\n{report}"
    );
    for backup in ["lib.bak-upgrade", "include.bak-upgrade"] {
        assert!(
            !home.path().join(backup).exists(),
            "`{backup}` should be removed once the new tree is verified\n{report}"
        );
    }
}

/// Run the real `moon upgrade` against the local server; returns success and
/// both streams, so a failing assertion can show what the user would have seen.
fn run_upgrade(home: &Path, url: &str) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_moon"))
        .args(["upgrade", "--force", "--non-interactive", "--base-url", url])
        .env("MOON_HOME", home)
        .env_remove("MOON_TOOLCHAIN_ROOT")
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .output()
        .expect("run moon upgrade");
    let report = format!(
        "--- status: {:?} ---\n--- stdout ---\n{}\n--- stderr ---\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), report)
}

/// The shipped installers' shape without the download: remove `lib` and
/// `include`, optionally leave a half-extracted core behind (files, no
/// manifest, which is what a kill during the core extraction produces), then
/// exit with `code`.
fn installer_script(code: i32, half_core: bool) -> String {
    if cfg!(windows) {
        let half = if half_core {
            "New-Item -ItemType Directory -Force -Path \"$env:MOON_HOME\\lib\\core\" | Out-Null\n\
             Set-Content -Path \"$env:MOON_HOME\\lib\\core\\partial-source.mbt\" -Value 'pub fn half() -> Unit {{}}'\n"
        } else {
            ""
        };
        format!(
            "if (Test-Path -LiteralPath \"$env:MOON_HOME\\lib\") {{ Remove-Item -Recurse -Force -LiteralPath \"$env:MOON_HOME\\lib\" }}\n\
             if (Test-Path -LiteralPath \"$env:MOON_HOME\\include\") {{ Remove-Item -Recurse -Force -LiteralPath \"$env:MOON_HOME\\include\" }}\n\
             {half}New-Item -ItemType File -Path \"$env:MOON_HOME\\installer-ran\" -Force | Out-Null\n\
             exit {code}\n"
        )
    } else {
        let half = if half_core {
            "mkdir -p \"$MOON_HOME/lib/core\"\n\
             printf 'pub fn half() -> Unit {{}}\\n' > \"$MOON_HOME/lib/core/partial-source.mbt\"\n"
        } else {
            ""
        };
        format!(
            "rm -rf \"$MOON_HOME/lib\" \"$MOON_HOME/include\"\n\
             {half}: > \"$MOON_HOME/installer-ran\"\nexit {code}\n"
        )
    }
}

/// The state a kill during the core extraction leaves behind: the previous tree
/// is in the backup, the "replacement" is a half-extracted core — files, no
/// manifest. Recovery must put the backup back; reading "lib/core has entries"
/// as a finished install deletes the only good copy.
#[test]
fn interrupted_upgrade_restores_the_backup_instead_of_a_partial_core() {
    let home = tempfile::tempdir().expect("toolchain tempdir");
    write_previous_toolchain(home.path());
    std::fs::rename(home.path().join("lib"), home.path().join("lib.bak-upgrade"))
        .expect("move the previous lib aside");
    std::fs::create_dir_all(home.path().join("lib/core")).expect("partial core dir");
    std::fs::write(
        home.path().join("lib/core/partial-source.mbt"),
        "pub fn half() -> Unit {}\n",
    )
    .expect("write the partial core");

    let server = ScriptServer::start(&installer_script(1, false));
    let (ok, report) = run_upgrade(home.path(), &server.url);

    assert!(
        home.path().join("installer-ran").is_file(),
        "the installer script never ran, so nothing was exercised\n{report}"
    );
    assert!(
        !ok,
        "a failing installer must fail `moon upgrade`\n{report}"
    );
    assert!(
        home.path().join("lib/core/previous-install").is_file(),
        "the only good backup was discarded instead of restored\n{report}"
    );
    assert!(
        !home.path().join("lib/core/partial-source.mbt").exists(),
        "the half-extracted core came back as the previous toolchain\n{report}"
    );
    for backup in ["lib.bak-upgrade", "include.bak-upgrade"] {
        assert!(
            !home.path().join(backup).exists(),
            "`{backup}` should be consumed by the restore\n{report}"
        );
    }
}

/// An interrupted install can stop between replacing `lib` and `include`. A
/// complete `lib` next to a missing `include` is two versions in one tree, so
/// the whole previous pair comes back rather than just the missing half.
#[test]
fn interrupted_upgrade_does_not_mix_a_new_lib_with_the_old_include() {
    let home = tempfile::tempdir().expect("toolchain tempdir");
    write_previous_toolchain(home.path());
    std::fs::rename(home.path().join("lib"), home.path().join("lib.bak-upgrade"))
        .expect("move the previous lib aside");
    std::fs::create_dir_all(home.path().join("lib/core")).expect("new lib dir");
    std::fs::write(
        home.path().join("lib/core/moon.mod"),
        "name = \"moonbitlang/core\"\n",
    )
    .expect("write the new manifest");
    std::fs::write(
        home.path().join("lib/core/new-install"),
        "from the new tree\n",
    )
    .expect("write the new marker");
    std::fs::rename(
        home.path().join("include"),
        home.path().join("include.bak-upgrade"),
    )
    .expect("move the previous include aside");

    let server = ScriptServer::start(&installer_script(1, false));
    let (ok, report) = run_upgrade(home.path(), &server.url);

    assert!(
        home.path().join("installer-ran").is_file(),
        "the installer script never ran, so nothing was exercised\n{report}"
    );
    assert!(
        !ok,
        "a failing installer must fail `moon upgrade`\n{report}"
    );
    assert!(
        home.path().join("lib/core/previous-install").is_file(),
        "the previous `lib` was not restored next to the previous `include`\n{report}"
    );
    assert!(
        !home.path().join("lib/core/new-install").exists(),
        "a new `lib` was kept next to the old `include`\n{report}"
    );
    assert!(
        home.path().join("include/moonbit.h").is_file(),
        "`include` was not restored\n{report}"
    );
}

/// A zero exit status from the installer is not evidence of an install: the
/// generated wrapper has no `$ErrorActionPreference`, so a failed download
/// still exits 0.
#[test]
fn a_zero_exit_status_does_not_cover_an_incomplete_tree() {
    let home = tempfile::tempdir().expect("toolchain tempdir");
    write_previous_toolchain(home.path());

    let server = ScriptServer::start(&installer_script(0, true));
    let (ok, report) = run_upgrade(home.path(), &server.url);

    assert!(
        home.path().join("installer-ran").is_file(),
        "the installer script never ran, so nothing was exercised\n{report}"
    );
    assert!(
        !ok,
        "an incomplete tree must fail the upgrade even when the installer exits 0\n{report}"
    );
    assert!(
        report.contains("incomplete toolchain"),
        "the failure should name the incomplete toolchain\n{report}"
    );
    assert!(
        home.path().join("lib/core/previous-install").is_file(),
        "the previous tree was not restored over the incomplete replacement\n{report}"
    );
}

/// The third gate check (`bin/moonc`) has to be live: `bin` is never backed up,
/// so a replacement that ships no `moonc` must still be rejected.
#[test]
fn a_replacement_without_moonc_is_rejected() {
    let home = tempfile::tempdir().expect("toolchain tempdir");
    write_previous_toolchain(home.path());

    let exe = std::env::consts::EXE_SUFFIX;
    let script = if cfg!(windows) {
        format!(
            r#"
New-Item -ItemType Directory -Force -Path "$env:MOON_HOME\lib\core", "$env:MOON_HOME\include" | Out-Null
Set-Content -Path "$env:MOON_HOME\lib\core\moon.mod" -Value 'name = "moonbitlang/core"'
Set-Content -Path "$env:MOON_HOME\include\moonbit.h" -Value '/* new */'
Remove-Item -Force -LiteralPath "$env:MOON_HOME\bin\moonc{exe}"
New-Item -ItemType File -Path "$env:MOON_HOME\installer-ran" -Force | Out-Null
exit 0
"#
        )
    } else {
        format!(
            r#"
mkdir -p "$MOON_HOME/lib/core" "$MOON_HOME/include"
printf 'name = "moonbitlang/core"\n' > "$MOON_HOME/lib/core/moon.mod"
printf '/* new */\n' > "$MOON_HOME/include/moonbit.h"
rm -f "$MOON_HOME/bin/moonc{exe}"
: > "$MOON_HOME/installer-ran"
exit 0
"#
        )
    };
    let server = ScriptServer::start(&script);
    let (ok, report) = run_upgrade(home.path(), &server.url);

    assert!(
        home.path().join("installer-ran").is_file(),
        "the installer script never ran, so nothing was exercised\n{report}"
    );
    assert!(
        !ok,
        "a replacement without `bin/moonc` must not be accepted\n{report}"
    );
    assert!(
        report.contains("bin/moonc"),
        "the failure should name the missing `bin/moonc`\n{report}"
    );
}
