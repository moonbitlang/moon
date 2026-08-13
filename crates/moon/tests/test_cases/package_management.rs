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

use super::*;

#[test]
fn mooncakes_io_smoke_test() {
    if std::env::var("CI").is_err() {
        return;
    }
    let dir = TestDir::new("hello");
    let _ = get_stdout(&dir, ["update"]);
    let _ = get_stdout(&dir, ["add", "lijunchen/hello2@0.1.0"]);
    check(
        std::fs::read_to_string(dir.join("moon.mod.json")).unwrap(),
        expect![[r#"
            {
              "name": "hello",
              "deps": {
                "lijunchen/hello2": "0.1.0"
              }
            }"#]],
    );
    let _ = get_stdout(&dir, ["remove", "lijunchen/hello2"]);
    check(
        std::fs::read_to_string(dir.join("moon.mod.json")).unwrap(),
        expect![[r#"
            {
              "name": "hello",
              "deps": {}
            }"#]],
    );
    let _ = get_stdout(&dir, ["add", "lijunchen/hello2@0.1.0"]);
    std::fs::write(
        dir.join("main/main.mbt"),
        r#"fn main {
  println(@lib.hello2())
}
"#,
    )
    .unwrap();

    let mooncakes_dir = dir.as_ref().join(".mooncakes");

    assert!(
        mooncakes_dir
            .join("lijunchen")
            .join("hello")
            .join(MOON_MOD_JSON)
            .exists()
    );

    std::fs::remove_dir_all(&mooncakes_dir).unwrap();
    let assert = moon_cmd(&dir).arg("install").assert().success();
    let output = assert.get_output();
    assert!(output.stdout.is_empty());
    let mut lines = std::str::from_utf8(&output.stderr)
        .unwrap()
        .lines()
        .filter(|line| line.starts_with("Using cached "))
        .collect::<Vec<_>>();
    lines.sort();
    check(
        lines.join("\n"),
        expect![[r#"
            Using cached lijunchen/hello2@0.1.0
            Using cached lijunchen/hello@0.1.0"#]],
    );

    std::fs::write(
        dir.join("main/moon.pkg.json"),
        r#"{
          "is-main": true,
          "import": [
            "lijunchen/hello2/lib"
          ]
        }
    "#,
    )
    .unwrap();

    std::fs::remove_dir_all(&mooncakes_dir).unwrap();
    let assert = moon_cmd(&dir).args(["run", "main"]).assert().success();
    check(
        std::str::from_utf8(&assert.get_output().stdout).unwrap(),
        expect![[r#"
            Hello, world!Hello, world2!
        "#]],
    );
    let stderr = std::str::from_utf8(&assert.get_output().stderr).unwrap();
    assert!(
        !stderr.contains("Using cached ") && !stderr.contains("Downloading "),
        "moon run should keep dependency sync quiet by default, got:\n{stderr}"
    );
}

#[test]
#[ignore = "where to download mooncake?"]
fn mooncake_cli_smoke_test() {
    let dir = TestDir::new("hello.in");
    let out = moon_process_cmd(&dir)
        .env("RUST_BACKTRACE", "0")
        .args(["publish"])
        .output()
        .unwrap();
    let s = std::str::from_utf8(&out.stderr).unwrap().to_string();
    assert!(s.contains("failed to open credentials file"));
}

#[test]
fn test_moon_update_reclones_for_different_registry_url() {
    if std::env::var("CI").is_err() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let moon_home = dir;
    moon_cmd(&dir)
        .env("MOON_HOME", moon_home)
        .args(["update"])
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq("Registry index cloned successfully\nSymbols updated successfully\n");

    let registry_index = moonutil::MoonHomeLayout::new(dir.to_path_buf()).registry_index_dir();
    let _ = std::process::Command::new("git")
        .args([
            "-C",
            registry_index.to_str().unwrap(),
            "remote",
            "set-url",
            "origin",
            "whatever",
        ])
        .output()
        .unwrap();

    moon_cmd(&dir)
        .env("MOON_HOME", moon_home)
        .args(["update"])
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq(
            "Registry index remote does not match the configured URL, re-cloning\nRegistry index re-cloned successfully\nSymbols updated successfully\n",
        );

    moon_cmd(&dir)
        .env("MOON_HOME", moon_home)
        .args(["update", "--quiet"])
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq("");
}

#[test]
fn test_moon_package_list() {
    let dir = TestDir::new("test_publish.in");
    check(
        get_stderr(&dir, ["package", "--list"]),
        expect![[r#"
            Running moon check ...
            Finished. moon: ran 4 tasks, now up to date
            Check passed
            README.md
            moon.mod.json
            src/lib/hello.mbt
            src/lib/hello_test.mbt
            src/lib/moon.pkg.json
            src/main/main.mbt
            src/main/moon.pkg.json
            Package to $ROOT/_build/publish/username-hello-0.1.0.zip
        "#]],
    );
}

#[test]
#[allow(clippy::just_underscores_and_digits)]
fn test_moon_install_bin() {
    struct BinFileCleanup(Vec<std::path::PathBuf>);

    impl Drop for BinFileCleanup {
        fn drop(&mut self) {
            for path in &self.0 {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    let top_dir = TestDir::new("moon_install_bin.in");
    let dir = top_dir.join("user.in");

    let installed_bins;

    #[cfg(unix)]
    {
        installed_bins = vec![
            top_dir.join("author2.in").join("author2-native"),
            top_dir.join("author2.in").join("author2-js"),
            top_dir.join("author2.in").join("author2-wasm"),
            top_dir.join("author1.in").join("this-is-wasm"),
            top_dir.join("author1.in").join("main-js"),
        ];
    }

    #[cfg(target_os = "windows")]
    {
        installed_bins = vec![
            top_dir.join("author2.in").join("author2-native.ps1"),
            top_dir.join("author2.in").join("author2-js.ps1"),
            top_dir.join("author2.in").join("author2-wasm.ps1"),
            top_dir.join("author1.in").join("this-is-wasm.ps1"),
            top_dir.join("author1.in").join("main-js.ps1"),
        ];
    }
    let _cleanup = BinFileCleanup(installed_bins.clone());

    // moon check should auto install bin deps
    get_stdout(&dir, ["check"]);
    for bin in &installed_bins {
        assert!(bin.exists());
    }

    {
        // delete all bin files
        for bin in &installed_bins {
            std::fs::remove_file(bin).unwrap();
        }
        for bin in &installed_bins {
            assert!(!bin.exists());
        }
    }

    // moon install should install bin deps
    get_stdout(&dir, ["install"]);

    for bin in &installed_bins {
        assert!(bin.exists());
    }

    let content = get_stderr(&dir, ["build", "--sort-input"]);

    // Ensure the prebuild tasks' outputs are shown
    assert!(content.contains("main-js"));
    assert!(content.contains("lib Hello, world!"));
    assert!(content.contains("()"));
}

#[test]
fn test_upgrade() -> anyhow::Result<()> {
    if std::env::var("CI").is_err() {
        return Ok(());
    }
    let tmp_dir = tempfile::TempDir::new()?;
    let _ = std::process::Command::new(moon_bin())
        .env("MOON_HOME", tmp_dir.path().to_str().unwrap())
        .env("MOON_TOOLCHAIN_ROOT", tmp_dir.path().to_str().unwrap())
        .arg("upgrade")
        .arg("--force")
        .arg("--non-interactive")
        .arg("--base-url")
        .arg("https://cli.moonbitlang.com")
        .output()?;
    #[cfg(unix)]
    let xs = [
        tmp_dir.path().join("bin").join("moon").exists(),
        tmp_dir.path().join("bin").join("moonc").exists(),
    ];
    #[cfg(windows)]
    let xs = [
        tmp_dir.path().join("bin").join("moon.exe").exists(),
        tmp_dir.path().join("bin").join("moonc.exe").exists(),
    ];
    check(format!("{xs:?}"), expect!["[true, true]"]);
    Ok(())
}

#[test]
fn test_upgrade_refuses_split_toolchain_root() -> anyhow::Result<()> {
    let dir = TestDir::new_empty();
    let moon_home = tempfile::TempDir::new()?;
    let toolchain_root = tempfile::TempDir::new()?;

    let stderr = get_err_stderr_with_envs(
        &dir,
        [
            "upgrade",
            "--force",
            "--non-interactive",
            "--base-url",
            "https://example.invalid",
        ],
        [
            ("MOON_HOME", moon_home.path().to_str().unwrap()),
            (
                "MOON_TOOLCHAIN_ROOT",
                toolchain_root.path().to_str().unwrap(),
            ),
        ],
    );

    assert!(stderr.contains("moon upgrade only supports toolchains installed under MOON_HOME."));
    assert!(stderr.contains(
        "Please upgrade this installation with the package manager or installer that owns the toolchain."
    ));
    Ok(())
}

#[test]
fn test_postadd_script() {
    if std::env::var("CI").is_err() {
        return;
    }
    let dir = TestDir::new("test_postadd_script.in");
    let output = get_stdout(&dir, ["add", "lijunchen/test_postadd"]);
    assert!(output.contains(".mooncakes/lijunchen/test_postadd"));

    let _ = get_stdout(&dir, ["remove", "lijunchen/test_postadd"]);

    let out = moon_process_cmd(&dir)
        .env("MOON_IGNORE_POSTADD", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .args(["add", "lijunchen/test_postadd"])
        .output()
        .unwrap();
    let out = String::from_utf8(out.stderr).unwrap();
    assert!(!out.contains(".mooncakes/lijunchen/test_postadd"));
}

#[test]
fn test_fetch_and_binary_install_run_legacy_postadd() {
    let dir = TestDir::new_empty();
    let moon_home = tempfile::TempDir::new().expect("failed to create temporary MOON_HOME");
    let registry = tempfile::TempDir::new().expect("failed to create temporary registry");
    let registry_index = registry.path().join("git/index");
    std::fs::create_dir_all(registry_index.parent().unwrap()).unwrap();
    let output = std::process::Command::new("git")
        .args(["init", "--bare", "--initial-branch=main"])
        .arg(&registry_index)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "failed to initialize local registry index:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let local_index =
        moonutil::MoonHomeLayout::new(moon_home.path().to_path_buf()).registry_index_dir();
    std::fs::create_dir_all(local_index.parent().unwrap()).unwrap();
    let output = std::process::Command::new("git")
        .arg("clone")
        .arg(&registry_index)
        .arg(&local_index)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "failed to clone local registry index:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let moon_path = std::env::join_paths(
        std::iter::once(
            moon_bin()
                .parent()
                .expect("test moon binary should have a parent directory")
                .to_path_buf(),
        )
        .chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )),
    )
    .expect("test PATH should be valid");
    let manifest = serde_json::json!({
        "name": "testuser/postadd",
        "version": "1.0.0",
        "source": "src",
        "scripts": {
            "postadd": "moon tool write-tcc-rsp-file src/tool/generated.mbt fn generated()->Int{42}"
        }
    })
    .to_string()
    .into_bytes();
    cache_registry_package(
        moon_home.path(),
        "testuser/postadd",
        "1.0.0",
        &[
            ("moon.mod.json", manifest),
            ("src/tool/moon.pkg.json", br#"{"is-main":true}"#.to_vec()),
            (
                "src/tool/main.mbt",
                br#"fn main { println(generated()) }"#.to_vec(),
            ),
        ],
    );
    let output = std::process::Command::new("git")
        .current_dir(&local_index)
        .args(["add", "."])
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = std::process::Command::new("git")
        .current_dir(&local_index)
        .args([
            "-c",
            "user.name=Moon Test",
            "-c",
            "user.email=moon-test@example.com",
            "commit",
            "-m",
            "Add fixture package",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "failed to commit local registry index:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = std::process::Command::new("git")
        .current_dir(&local_index)
        .args(["push", "origin", "main"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "failed to publish local registry index:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(
        moon_home.path().join("config.json"),
        serde_json::json!({
            "registry": "https://registry.invalid",
            "index": registry_index,
        })
        .to_string(),
    )
    .unwrap();

    moon_cmd(&dir)
        .env("MOON_HOME", moon_home.path())
        .env("PATH", &moon_path)
        .args(["fetch", "--no-update", "testuser/postadd@1.0.0"])
        .assert()
        .success();
    assert!(
        dir.join(".repos/testuser/postadd/1.0.0/src/tool/generated.mbt")
            .is_file()
    );

    let bin_dir = dir.join("bin");
    let install = moon_cmd(&dir)
        .env("MOON_HOME", moon_home.path())
        .env("PATH", &moon_path)
        .args(["install", "testuser/postadd/tool@1.0.0", "--bin"])
        .arg(&bin_dir)
        .assert()
        .success();
    let stderr = String::from_utf8_lossy(&install.get_output().stderr);
    assert!(
        stderr.contains("Registry index updated successfully"),
        "local registry update did not succeed:\n{stderr}"
    );
    #[cfg(unix)]
    assert!(bin_dir.join("tool").is_file());
    #[cfg(windows)]
    assert!(bin_dir.join("tool.exe").is_file());
}
