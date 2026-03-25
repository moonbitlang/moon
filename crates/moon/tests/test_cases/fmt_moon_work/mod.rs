use super::*;

#[test]
fn test_fmt_existing_moon_work_dry_run() {
    let dir = TestDir::new("fmt_moon_work_existing.in");

    assert!(dir.join("moon.work").exists());

    check(
        get_stdout(&dir, ["fmt", "--dry-run", "--sort-input"]),
        expect![[r#"
            moon tool format-workspace --old ./moon.work --write --new ./_build/wasm-gc/release/format/moon.work
            moonfmt ./app/main/moon.pkg -w -o ./_build/wasm-gc/release/format/main/moon.pkg
            moonfmt ./app/main/main.mbt -w -o ./_build/wasm-gc/release/format/main/main.mbt
        "#]],
    );
}

#[test]
fn test_fmt_existing_moon_work_formats_in_place() {
    let dir = TestDir::new("fmt_moon_work_existing.in");

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["fmt"])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(dir.join("moon.work")).unwrap(),
        "members = [\n  \"./app\",\n]\npreferred_target = \"wasm-gc\"\n"
    );
}

#[test]
fn test_fmt_moon_work_json_migration_dry_run() {
    let dir = TestDir::new("fmt_moon_work.in");

    assert!(dir.join("moon.work.json").exists());

    check(
        get_stderr(
            &dir,
            [
                "--unstable-feature",
                "rr_moon_pkg",
                "fmt",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![""],
    );

    let output = get_stdout(
        &dir,
        [
            "--unstable-feature",
            "rr_moon_pkg",
            "fmt",
            "--dry-run",
            "--sort-input",
        ],
    );

    if cfg!(windows) {
        check(
            output,
            expect![[r#"
                moon tool format-workspace --old ./moon.work.json --new ./_build/wasm-gc/release/format/moon.work
                cmd /c copy ./_build/wasm-gc/release/format/moon.work ./moon.work
                cmd /c del ./moon.work.json
                moonfmt ./app/main/moon.pkg -w -o ./_build/wasm-gc/release/format/main/moon.pkg
                moonfmt ./app/main/main.mbt -w -o ./_build/wasm-gc/release/format/main/main.mbt
            "#]],
        );
    } else {
        check(
            output,
            expect![[r#"
                moon tool format-workspace --old ./moon.work.json --new ./_build/wasm-gc/release/format/moon.work
                cp ./_build/wasm-gc/release/format/moon.work ./moon.work
                rm ./moon.work.json
                moonfmt ./app/main/moon.pkg -w -o ./_build/wasm-gc/release/format/main/moon.pkg
                moonfmt ./app/main/main.mbt -w -o ./_build/wasm-gc/release/format/main/main.mbt
            "#]],
        );
    }
}

#[test]
fn test_fmt_moon_work_json_migration_removes_legacy_file() {
    let dir = TestDir::new("fmt_moon_work.in");

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["--unstable-feature", "rr_moon_pkg", "fmt"])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(dir.join("moon.work")).unwrap(),
        "members = [\n  \"./app\",\n]\npreferred_target = \"wasm-gc\"\n"
    );
    assert!(!dir.join("moon.work.json").exists());
}
