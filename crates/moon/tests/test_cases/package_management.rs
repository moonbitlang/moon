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
    let out = get_stdout(&dir, ["install"]);
    let mut lines = out.lines().collect::<Vec<_>>();
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

    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!Hello, world2!
        "#]],
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
fn test_moon_update_failed() {
    if std::env::var("CI").is_err() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let moon_home = dir;
    let out = moon_process_cmd(&dir)
        .env("MOON_HOME", moon_home)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .args(["update"])
        .output()
        .unwrap();
    let out = String::from_utf8(out.stderr).unwrap();
    check(
        &out,
        expect![[r#"
            Registry index cloned successfully
            Symbols updated successfully
        "#]],
    );

    let _ = std::process::Command::new("git")
        .args([
            "-C",
            dir.join("registry").join("index").to_str().unwrap(),
            "remote",
            "set-url",
            "origin",
            "whatever",
        ])
        .output()
        .unwrap();

    let out = moon_process_cmd(&dir)
        .env("MOON_HOME", moon_home)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .args(["update"])
        .output()
        .unwrap();
    let out = String::from_utf8(out.stderr).unwrap();
    check(
        &out,
        expect![[r#"
            Registry index is not cloned from the same URL, re-cloning
            Registry index re-cloned successfully
            Symbols updated successfully
        "#]],
    );
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
            src
            src/lib
            src/lib/hello.mbt
            src/lib/hello_test.mbt
            src/lib/moon.pkg.json
            src/main
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
    let dir = TestDir::new("test_postadd_script.in");
    let moon_home = tempfile::TempDir::new().unwrap();
    registry::cache_package(
        moon_home.path(),
        serde_json::json!({
            "name": "testuser/postadd",
            "version": "1.2.3",
            "scripts": { "postadd": format!("{} version", moon_bin().display()) },
        }),
        [] as [(&str, &[u8]); 0],
    );

    let executed = moon_process_cmd(&dir)
        .env("MOON_HOME", moon_home.path())
        .args(["add", "--quiet", "--no-update", "testuser/postadd@1.2.3"])
        .output()
        .unwrap();
    assert!(executed.status.success(), "{executed:?}");
    assert!(!executed.stdout.is_empty(), "postadd should execute once");
    snapbox::assert_data_eq!(
        String::from_utf8_lossy(&executed.stderr).as_ref(),
        snapbox::str![[r#"
Warning: Package `testuser/postadd@1.2.3` declares deprecated `scripts.postadd`; explicit `moon add` may still execute it temporarily for compatibility (set `MOON_IGNORE_POSTADD` to skip)

"#]],
    );

    let ignored_dir = TestDir::new("test_postadd_script.in");
    let ignored = moon_process_cmd(&ignored_dir)
        .env("MOON_HOME", moon_home.path())
        .env("MOON_IGNORE_POSTADD", "1")
        .args(["add", "--quiet", "--no-update", "testuser/postadd@1.2.3"])
        .output()
        .unwrap();
    assert!(ignored.status.success(), "{ignored:?}");
    assert!(
        ignored.stdout.is_empty(),
        "ignored postadd must not execute"
    );
    snapbox::assert_data_eq!(
        String::from_utf8_lossy(&ignored.stderr).as_ref(),
        snapbox::str![[r#"
Warning: Package `testuser/postadd@1.2.3` declares deprecated `scripts.postadd`; explicit `moon add` may still execute it temporarily for compatibility (set `MOON_IGNORE_POSTADD` to skip)

"#]],
    );
}

#[test]
fn fetch_warns_and_skips_deprecated_postadd() {
    let dir = TestDir::new_empty();
    let moon_home = tempfile::TempDir::new().unwrap();
    registry::cache_package(
        moon_home.path(),
        serde_json::json!({
            "name": "testuser/postadd",
            "version": "1.2.3",
            "scripts": { "postadd": "postadd-must-not-run" },
        }),
        [("README.md", b"fetched")],
    );

    let output = moon_process_cmd(&dir)
        .env("MOON_HOME", moon_home.path())
        .args(["fetch", "--quiet", "--no-update", "testuser/postadd@1.2.3"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty());
    snapbox::assert_data_eq!(
        String::from_utf8_lossy(&output.stderr).as_ref(),
        snapbox::str![[r#"
Warning: Package `testuser/postadd@1.2.3` declares deprecated `scripts.postadd`; the hook was not executed

"#]],
    );
    assert_eq!(
        std::fs::read_to_string(dir.join(".repos/testuser/postadd/1.2.3/README.md")).unwrap(),
        "fetched"
    );
}

#[test]
fn dependency_sync_warns_and_skips_deprecated_postadd() {
    let dir = TestDir::new_empty();
    std::fs::write(
        dir.join("moon.mod.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "name": "testuser/project",
            "version": "0.1.0",
            "deps": { "testuser/postadd": "1.2.3" },
        }))
        .unwrap(),
    )
    .unwrap();
    let moon_home = tempfile::TempDir::new().unwrap();
    registry::cache_package(
        moon_home.path(),
        serde_json::json!({
            "name": "testuser/postadd",
            "version": "1.2.3",
            "scripts": { "postadd": "postadd-must-not-run" },
        }),
        [] as [(&str, &[u8]); 0],
    );

    let output = moon_process_cmd(&dir)
        .env("MOON_HOME", moon_home.path())
        .args(["install", "--quiet"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty());
    snapbox::assert_data_eq!(
        String::from_utf8_lossy(&output.stderr).as_ref(),
        snapbox::str![[r#"
Warning: `moon install` without arguments is deprecated and will be removed in a future version. Use `moon install <package>` to install binaries globally, or use `moon build` to build your project.
Warning: Package `testuser/postadd@1.2.3` declares deprecated `scripts.postadd`; the hook was not executed

"#]],
    );
    assert!(
        dir.join(".mooncakes/testuser/postadd/moon.mod.json")
            .is_file()
    );
}
