use super::*;

#[test]
fn test_fmt_existing_moon_mod_dry_run() {
    let dir = TestDir::new("fmt_moon_mod_existing.in");

    assert!(dir.join("moon.mod").exists());

    check(
        get_stderr(&dir, ["fmt", "--dry-run", "--sort-input"]),
        expect![[r#""#]],
    );

    check(
        get_stdout(&dir, ["fmt", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonfmt ./main/moon.pkg -w -o ./_build/wasm-gc/release/format/main/moon.pkg
            moonfmt ./moon.mod -w -o ./_build/wasm-gc/release/format/moon.mod
            moonfmt ./main/main.mbt -w -o ./_build/wasm-gc/release/format/main/main.mbt
        "#]],
    );
}

#[test]
fn test_fmt_existing_moon_mod_formats_in_place() {
    let dir = TestDir::new("fmt_moon_mod_existing.in");

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["fmt"])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(dir.join("moon.mod")).unwrap(),
        r#"name = "test/fmt_moon_mod"

version = "0.0.1"

import {
  "example/dep@0.1.0",
}

options(
  readme: "README.md",
  supported_targets: [ "wasm-gc", "js" ],
)
"#
    );
}

#[test]
fn test_fmt_without_moon_mod_feature() {
    let dir = TestDir::new("fmt_moon_mod.in");

    check(
        get_stderr(&dir, ["fmt", "--dry-run", "--sort-input"]),
        expect![[r#""#]],
    );

    check(
        get_stdout(&dir, ["fmt", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonfmt ./main/moon.pkg -w -o ./_build/wasm-gc/release/format/main/moon.pkg
            moonfmt ./main/main.mbt -w -o ./_build/wasm-gc/release/format/main/main.mbt
        "#]],
    );
}

#[test]
fn test_fmt_without_moon_mod_feature_keeps_legacy_file() {
    let dir = TestDir::new("fmt_moon_mod.in");
    let original = std::fs::read_to_string(dir.join("moon.mod.json")).unwrap();

    assert_success(&dir, ["fmt"]);

    assert!(!dir.join("moon.mod").exists());
    assert!(dir.join("moon.mod.json").exists());
    assert_eq!(
        std::fs::read_to_string(dir.join("moon.mod.json")).unwrap(),
        original
    );
}

#[test]
fn test_fmt_moon_mod_json_migration_dry_run() {
    let dir = TestDir::new("fmt_moon_mod.in");

    assert!(dir.join("moon.mod.json").exists());

    check(
        get_stderr(
            &dir,
            [
                "--unstable-feature",
                "rr_moon_mod",
                "fmt",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            Warning: Migrating to moon.mod at module root '$ROOT', deprecated moon.mod.json is removed.
        "#]],
    );

    let output = get_stdout(
        &dir,
        [
            "--unstable-feature",
            "rr_moon_mod",
            "fmt",
            "--dry-run",
            "--sort-input",
        ],
    );

    if cfg!(windows) {
        check(
            output,
            expect![[r#"
                moonfmt ./main/moon.pkg -w -o ./_build/wasm-gc/release/format/main/moon.pkg
                moonfmt ./moon.mod.json -o ./_build/wasm-gc/release/format/moon.mod
                cmd /c copy ./_build/wasm-gc/release/format/moon.mod ./moon.mod
                cmd /c del ./moon.mod.json
                moonfmt ./main/main.mbt -w -o ./_build/wasm-gc/release/format/main/main.mbt
            "#]],
        );
    } else {
        check(
            output,
            expect![[r#"
                moonfmt ./main/moon.pkg -w -o ./_build/wasm-gc/release/format/main/moon.pkg
                moonfmt ./moon.mod.json -o ./_build/wasm-gc/release/format/moon.mod
                cp ./_build/wasm-gc/release/format/moon.mod ./moon.mod
                rm ./moon.mod.json
                moonfmt ./main/main.mbt -w -o ./_build/wasm-gc/release/format/main/main.mbt
            "#]],
        );
    }
}

#[test]
fn test_fmt_moon_mod_both_exist() {
    let dir = TestDir::new("fmt_moon_mod_both.in");

    check(
        get_stderr(&dir, ["fmt", "--dry-run", "--sort-input"]),
        expect![[r#"
            Warning: Both moon.mod.json and moon.mod exist at module root '$ROOT', using the new format moon.mod. Please remove the deprecated moon.mod.json.
        "#]],
    );

    check(
        get_stdout(&dir, ["fmt", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonfmt ./main/moon.pkg -w -o ./_build/wasm-gc/release/format/main/moon.pkg
            moonfmt ./moon.mod -w -o ./_build/wasm-gc/release/format/moon.mod
            moonfmt ./main/main.mbt -w -o ./_build/wasm-gc/release/format/main/main.mbt
        "#]],
    );
}

#[test]
fn test_fmt_moon_mod_both_exist_formats_moon_mod_in_place() {
    let dir = TestDir::new("fmt_moon_mod_both.in");
    let original_json = std::fs::read_to_string(dir.join("moon.mod.json")).unwrap();

    let stderr = get_stderr(&dir, ["fmt"]);
    assert!(
        stderr.contains("Both moon.mod.json and moon.mod exist"),
        "{stderr}"
    );

    assert!(dir.join("moon.mod").exists());
    assert!(dir.join("moon.mod.json").exists());
    assert_eq!(
        std::fs::read_to_string(dir.join("moon.mod.json")).unwrap(),
        original_json
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("moon.mod")).unwrap(),
        r#"name = "test/fmt_moon_mod_both"

version = "0.0.1"

options(
  readme: "README.md",
)
"#
    );
}

#[test]
fn test_fmt_moon_mod_json_migration_removes_legacy_file() {
    let dir = TestDir::new("fmt_moon_mod.in");

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["--unstable-feature", "rr_moon_mod", "fmt"])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(dir.join("moon.mod")).unwrap(),
        r#"name = "test/fmt_moon_mod_json"

version = "0.0.1"

import {
  "example/dep@0.1.0",
}

warnings = "+w1-w2"

options(
  readme: "README.md",
)"#
    );
    assert!(!dir.join("moon.mod.json").exists());
}

#[test]
fn test_fmt_moon_mod_local_deps_fail() {
    let dir = TestDir::new_empty();
    std::fs::create_dir_all(dir.join("main")).unwrap();
    std::fs::write(
        dir.join("moon.mod"),
        r#"name = "test/local_deps"

options(
  deps: {
    "example/local": { "path": "../local" },
  },
)
"#,
    )
    .unwrap();
    std::fs::write(dir.join("main/moon.pkg"), r#"options("is-main": true)"#).unwrap();
    std::fs::write(dir.join("main/main.mbt"), "fn main { println(1) }\n").unwrap();

    let stderr = get_err_stderr(&dir, ["fmt"]);
    assert!(
        stderr.contains("moon.mod does not support local dependency"),
        "{stderr}"
    );
    assert!(stderr.contains("moon.work"), "{stderr}");
}
