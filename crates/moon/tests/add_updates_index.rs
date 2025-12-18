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

use std::process::{Command, Stdio};

fn moon_bin() -> std::path::PathBuf {
    // NOTE: keep consistent with existing tests.
    snapbox::cargo_bin!("moon").to_owned()
}

fn git_writes_allowed() -> bool {
    // Some environments (e.g. sandboxed runners) disallow creating `.git` directories.
    // `moon update`/`git clone` needs that, so skip the test in such environments.
    let dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(_) => return false,
    };
    std::fs::create_dir(dir.path().join(".git")).is_ok()
}

fn init_empty_local_registry() -> tempfile::TempDir {
    // Setup (offline): create a local bare git repo to act as registry index.
    // This avoids network usage while still exercising `moon update` / registry clone logic.
    let base = tempfile::tempdir().unwrap();
    let index_repo = base.path().join("git").join("index");
    std::fs::create_dir_all(&index_repo).unwrap();
    let out = Command::new("git")
        .current_dir(&index_repo)
        .args(["init", "--bare"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert!(out.status.success());
    base
}

fn write_minimal_moon_mod_json(dir: &std::path::Path) {
    // NOTE: `moon add` only requires `moon.mod.json` to exist for project discovery.
    std::fs::write(
        dir.join("moon.mod.json"),
        r#"{"name":"test/empty","version":"0.0.1"}"#,
    )
    .unwrap();
}

#[test]
fn test_moon_add_no_update_skips_registry_update() {
    // Setup: isolated Moon project + empty MOON_HOME.
    let project = tempfile::tempdir().unwrap();
    write_minimal_moon_mod_json(project.path());

    let moon_home = tempfile::tempdir().unwrap();

    // Execute: ensure `--no-update` does not trigger registry index updates.
    let out = Command::new(moon_bin())
        .current_dir(project.path())
        .env("MOON_HOME", moon_home.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args([
            "add",
            "--no-update",
            "this_user_should_not_exist/this_pkg_should_not_exist",
        ])
        .output()
        .unwrap();

    // Assert: command should fail (package doesn't exist), and no registry update message appears.
    assert!(!out.status.success());
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!combined.contains("Registry index"));
}

#[test]
fn test_moon_add_updates_registry_index_by_default() {
    if !git_writes_allowed() {
        return;
    }

    // Setup (offline): use a local registry so the test does not depend on network.
    let registry_base = init_empty_local_registry();

    // Setup: isolated Moon project + fresh MOON_HOME.
    let project = tempfile::tempdir().unwrap();
    write_minimal_moon_mod_json(project.path());

    let moon_home = tempfile::tempdir().unwrap();

    // Execute: default `moon add` should update the registry index.
    // Use a non-existent package so we don't depend on downloading package artifacts.
    let out = Command::new(moon_bin())
        .current_dir(project.path())
        .env("MOON_HOME", moon_home.path())
        .env("MOONCAKES_REGISTRY", registry_base.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args([
            "add",
            "this_user_should_not_exist/this_pkg_should_not_exist",
        ])
        .output()
        .unwrap();

    // Assert: `moon update`-style clone message is printed.
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("Registry index cloned successfully"),
        "moon add output:\n{combined}"
    );
    // Assert: error message should NOT suggest `moon update` because it was just updated.
    assert!(
        !combined.contains("Please consider running `moon update`"),
        "error message should be concise when updated:\n{combined}"
    );
}

#[test]
fn test_moon_add_no_update_suggests_update_on_failure() {
    // Setup: isolated Moon project + empty MOON_HOME.
    let project = tempfile::tempdir().unwrap();
    write_minimal_moon_mod_json(project.path());

    let moon_home = tempfile::tempdir().unwrap();

    // Execute: `--no-update` with a non-existent package.
    let out = Command::new(moon_bin())
        .current_dir(project.path())
        .env("MOON_HOME", moon_home.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args([
            "add",
            "--no-update",
            "this_user_should_not_exist/this_pkg_should_not_exist",
        ])
        .output()
        .unwrap();

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // Assert: error message SHOULD suggest `moon update` because update was skipped.
    assert!(
        combined.contains("Please consider running `moon update`"),
        "error message should suggest update when skipped:\n{combined}"
    );
}
