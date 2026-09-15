use super::*;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[test]
fn test_fmt_mbtx_in_package() {
    let dir = TestDir::new_empty();
    let source = "///|\nfn main{println(1)}\n";
    std::fs::write(dir.join("moon.mod"), "name=\"test/fmt\"\n").unwrap();
    std::fs::write(dir.join("moon.pkg"), "\n").unwrap();
    std::fs::write(dir.join("build.mbtx"), source).unwrap();
    std::fs::write(dir.join("control.mbt"), source).unwrap();
    #[cfg(unix)]
    std::fs::set_permissions(
        dir.join("build.mbtx"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["fmt", "--check", "build.mbtx"])
        .assert()
        .failure();
    assert_eq!(read(dir.join("build.mbtx")), source);

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["fmt", "--warn", "build.mbtx"])
        .assert()
        .success()
        .stdout_eq("File not formatted: [..]/build.mbtx\n");
    assert_eq!(read(dir.join("build.mbtx")), source);

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["fmt", "build.mbtx"])
        .assert()
        .success();
    expect![[r#"
        ///|
        fn main {
          println(1)
        }
    "#]]
    .assert_eq(&read(dir.join("build.mbtx")));
    #[cfg(unix)]
    assert_eq!(
        std::fs::metadata(dir.join("build.mbtx"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert_eq!(read(dir.join("control.mbt")), source);
    assert_eq!(read(dir.join("moon.pkg")), "\n");
    assert_eq!(read(dir.join("moon.mod")), "name=\"test/fmt\"\n");

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["fmt", "--check", "build.mbtx"])
        .assert()
        .success();

    // Package discovery still excludes standalone scripts.
    std::fs::write(dir.join("build.mbtx"), source).unwrap();
    for args in [vec!["fmt"], vec!["fmt", "."]] {
        snapbox::cmd::Command::new(moon_bin())
            .current_dir(&dir)
            .args(args)
            .assert()
            .success();
        assert_eq!(read(dir.join("build.mbtx")), source);
    }
}

#[test]
fn test_fmt_mbtx_without_project() {
    let dir = TestDir::new_empty();
    let source = "import {\n  \"unavailable/dependency@0.1.0\",\n}\n\nfn main{println(1)}\n";
    for folder in ["first", "second"] {
        std::fs::create_dir(dir.join(folder)).unwrap();
        std::fs::write(dir.join(format!("{folder}/build.mbtx")), source).unwrap();
    }

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["fmt", "--dry-run", "first/build.mbtx", "second/build.mbtx"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
moonfmt ./first/build.mbtx -w -o ./first/_build/build.mbtx/format/build.mbtx
moonfmt ./second/build.mbtx -w -o ./second/_build/build.mbtx/format/build.mbtx

"#]]);

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args([
            "fmt",
            "first/build.mbtx",
            "second/build.mbtx",
            "--target-dir",
            "output",
        ])
        .assert()
        .success();
    let first = read(dir.join("first/build.mbtx"));
    assert_ne!(first, source);
    assert_eq!(read(dir.join("second/build.mbtx")), first);

    // Explicit scripts do not need the surrounding project's manifests to parse.
    std::fs::write(dir.join("moon.mod"), "not a valid manifest").unwrap();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["fmt", "--check", "first/build.mbtx", "second/build.mbtx"])
        .assert()
        .success();
    assert!(!dir.join(".mooncakes").exists());
}

#[test]
fn test_fmt_mbtx_validates_paths_before_formatting() {
    let dir = TestDir::new_empty();
    let source = "fn main{println(1)}\n";
    std::fs::write(dir.join("build.mbtx"), source).unwrap();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["fmt", "build.mbtx", "missing.mbtx"])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: failed to resolve file path `missing.mbtx`

Caused by:
    [..] (os error [..])

"#]]);
    assert_eq!(read(dir.join("build.mbtx")), source);

    std::fs::create_dir(dir.join("directory.mbtx")).unwrap();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["fmt", "build.mbtx", "directory.mbtx"])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: formatter input `directory.mbtx` must be a file

"#]]);
    assert_eq!(read(dir.join("build.mbtx")), source);
}

#[test]
fn test_fmt_mbtx_dry_run_and_mixed_paths() {
    let dir = TestDir::new_empty();
    let source = "fn main{println(1)}\n";
    std::fs::write(dir.join("moon.mod"), "name=\"test/fmt\"\n").unwrap();
    std::fs::write(dir.join("moon.pkg"), "\n").unwrap();
    std::fs::write(dir.join("build.mbtx"), source).unwrap();
    std::fs::write(dir.join("control.mbt"), source).unwrap();
    std::fs::create_dir(dir.join("notes")).unwrap();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args([
            "fmt",
            "--dry-run",
            "--check",
            "build.mbtx",
            "./build.mbtx",
            "notes",
            "--",
            "-strip-uuid",
        ])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
moon tool format-and-diff --old ./build.mbtx --new ./_build/build.mbtx/format/build.mbtx -- -strip-uuid

"#]]);

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["fmt", "--dry-run", "build.mbtx", "."])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
moonfmt ./build.mbtx -w -o ./_build/build.mbtx/format/build.mbtx
moonfmt ./moon.pkg -w -o ./_build/wasm-gc/release/format/moon.pkg
moonfmt ./control.mbt -w -o ./_build/wasm-gc/release/format/control.mbt

"#]]);
    assert_eq!(read(dir.join("build.mbtx")), source);
    assert_eq!(read(dir.join("control.mbt")), source);
}

#[test]
fn test_fmt_mbtx_dry_run_outside_current_directory() {
    let dir = TestDir::new_empty();
    std::fs::create_dir_all(dir.join("first/work")).unwrap();
    std::fs::create_dir(dir.join("second")).unwrap();
    for file in ["first/build.mbtx", "second/build.mbtx"] {
        std::fs::write(dir.join(file), "fn main{}\n").unwrap();
    }

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(dir.join("first/work"))
        .args([
            "fmt",
            "--dry-run",
            "../build.mbtx",
            "../../second/build.mbtx",
        ])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
moonfmt ../build.mbtx -w -o ../_build/build.mbtx/format/build.mbtx
moonfmt ../../second/build.mbtx -w -o ../../second/_build/build.mbtx/format/build.mbtx

"#]]);

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(dir.join("first/work"))
        .args(["fmt", "--dry-run", "--check", "../../second/build.mbtx"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
moon tool format-and-diff --old ../../second/build.mbtx --new ../../second/_build/build.mbtx/format/build.mbtx

"#]]);
}

#[test]
fn test_fmt_mbtx_dry_run_preserves_package_root() {
    let dir = TestDir::new_empty();
    std::fs::create_dir(dir.join("work")).unwrap();
    std::fs::write(dir.join("moon.mod"), "name=\"test/fmt\"\n").unwrap();
    std::fs::write(dir.join("moon.pkg"), "\n").unwrap();
    for file in ["build.mbtx", "control.mbt"] {
        std::fs::write(dir.join(file), "fn main{}\n").unwrap();
    }

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(dir.join("work"))
        .args(["fmt", "--dry-run", ".."])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
moonfmt ./moon.pkg -w -o ./_build/wasm-gc/release/format/moon.pkg
moonfmt ./control.mbt -w -o ./_build/wasm-gc/release/format/control.mbt

"#]]);

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(dir.join("work"))
        .args(["fmt", "--dry-run", "../build.mbtx", ".."])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
moonfmt ../build.mbtx -w -o ../_build/build.mbtx/format/build.mbtx
moonfmt ./moon.pkg -w -o ./_build/wasm-gc/release/format/moon.pkg
moonfmt ./control.mbt -w -o ./_build/wasm-gc/release/format/control.mbt

"#]]);
}

#[test]
fn test_fmt_mbtx_dry_run_dumps_all_plans() {
    let dir = TestDir::new_empty();
    std::fs::write(dir.join("moon.mod"), "name=\"test/fmt\"\n").unwrap();
    std::fs::write(dir.join("moon.pkg"), "\n").unwrap();
    for file in ["first.mbtx", "second.mbtx", "control.mbt"] {
        std::fs::write(dir.join(file), "fn main{}\n").unwrap();
    }
    let dump = dir.join("graph.jsonl");

    for (paths, expected) in [
        (
            ["first.mbtx", "second.mbtx"],
            vec![
                "moonfmt ./first.mbtx -w -o ./_build/first.mbtx/format/first.mbtx",
                "moonfmt ./second.mbtx -w -o ./_build/second.mbtx/format/second.mbtx",
            ],
        ),
        (
            ["first.mbtx", "."],
            vec![
                "moonfmt ./first.mbtx -w -o ./_build/first.mbtx/format/first.mbtx",
                "moonfmt ./control.mbt -w -o ./_build/wasm-gc/release/format/control.mbt",
                "moonfmt ./moon.pkg -w -o ./_build/wasm-gc/release/format/moon.pkg",
            ],
        ),
    ] {
        // Each invocation replaces an earlier dump, then accumulates its plans.
        std::fs::write(&dump, "stale dump\n").unwrap();
        snapbox::cmd::Command::new(moon_bin())
            .current_dir(&dir)
            .env("MOON_TEST_DUMP_BUILD_GRAPH", &dump)
            .args(["fmt", "--dry-run"])
            .args(paths)
            .assert()
            .success();
        let commands = read(&dump)
            .lines()
            .map(|line| {
                let node: serde_json::Value = serde_json::from_str(line).unwrap();
                node["command"].as_str().unwrap().to_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(commands, expected);
    }
}

#[test]
fn test_fmt_path() {
    let dir = TestDir::new("fmt_path.in");

    let _ = get_stdout(&dir, ["fmt", "."]);
    check(
        read(dir.join("fmt_path.mbt")),
        expect![[r#"
            ///|
            pub fn fib(n : Int) -> Int {
              n
            }
        "#]],
    );
    check(
        read(dir.join("cmd/main/main.mbt")),
        expect![[r#"
            ///|
            fn main { println(@lib.fib(10)) }
        "#]],
    );

    let _ = get_stdout(&dir, ["fmt", "cmd/main"]);
    check(
        read(dir.join("cmd/main/main.mbt")),
        expect![[r#"
            ///|
            fn main {
              println(@lib.fib(10))
            }
        "#]],
    );
}

#[test]
fn test_fmt_multiple_paths_skip_filtered_entries() {
    let dir = TestDir::new("fmt_path.in");
    std::fs::create_dir_all(dir.join("notes")).unwrap();
    std::fs::write(dir.join("notes/README.txt"), "not a package").unwrap();

    let _ = get_stdout(&dir, ["fmt", "fmt_path.mbt", "cmd/main", "notes"]);
    check(
        read(dir.join("fmt_path.mbt")),
        expect![[r#"
            ///|
            pub fn fib(n : Int) -> Int {
              n
            }
        "#]],
    );
    check(
        read(dir.join("cmd/main/main.mbt")),
        expect![[r#"
            ///|
            fn main {
              println(@lib.fib(10))
            }
        "#]],
    );

    let stderr = get_stderr(
        &dir,
        ["fmt", "fmt_path.mbt", "cmd/main", "notes", "--verbose"],
    );
    assert!(stderr.contains("skipping path `notes`"), "stderr: {stderr}");
}

#[test]
fn test_fmt_multiple_paths_skip_pkg_like_dirs_outside_source() {
    let dir = TestDir::new("path_outside_source.in");

    let _ = get_stdout(&dir, ["fmt", "src/main", "generated/ghost"]);
    check(
        read(dir.join("src/main/main.mbt")),
        expect![[r#"
            ///|
            fn main {
              println("hello")
            }
        "#]],
    );

    let stderr = get_stderr(&dir, ["fmt", "src/main", "generated/ghost", "--verbose"]);
    assert!(
        stderr.contains("skipping path `generated/ghost`"),
        "stderr: {stderr}"
    );
}
