use super::*;

#[test]
fn test_fmt_existing_moon_mod_dry_run() {
    let dir = TestDir::new("fmt_moon_mod_existing.in");

    assert!(dir.join("moon.mod").exists());

    moon_cmd(&dir)
        .args(["fmt", "--dry-run", "--sort-input", "--quiet"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
moonfmt ./main/moon.pkg -w -o ./_build/wasm-gc/release/format/main/moon.pkg
moonfmt ./moon.mod -w -o ./_build/wasm-gc/release/format/moon.mod
moonfmt ./main/main.mbt -w -o ./_build/wasm-gc/release/format/main/main.mbt

"#]])
        .stderr_eq("");
}

#[test]
fn test_check_existing_moon_mod_dry_run() {
    let dir = TestDir::new("fmt_moon_mod_existing.in");
    std::fs::write(
        dir.join("moon.mod"),
        "name = \"test/check_moon_mod\"\n\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    let output = get_stdout(&dir, ["check", "--dry-run", "--sort-input"]);

    assert!(output.contains("moonc check ./main/main.mbt"), "{output}");
}

#[test]
fn test_check_existing_moon_mod_supports_toplevel_config() {
    let dir = TestDir::new("fmt_moon_mod_existing.in");
    std::fs::write(
        dir.join("moon.mod"),
        r#"name = "test/check_moon_mod"

version = "0.1.0"

readme = "README.md"
repository = "https://example.com/check_moon_mod"
license = "Apache-2.0"
keywords = ["moonbit", "dsl"]
description = "Module metadata"
preferred_target = "js"
supported_targets = "js"
"#,
    )
    .unwrap();

    let output = get_stdout(&dir, ["check", "--dry-run", "--sort-input"]);

    assert!(output.contains("moonc check ./main/main.mbt"), "{output}");
    assert!(output.contains("./_build/js/debug/check/main"), "{output}");
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

readme = "README.md"

supported_targets = [ "wasm-gc", "js" ]
"#
    );
}

#[test]
fn test_fmt_moon_mod_json_migration_dry_run() {
    let dir = TestDir::new("fmt_moon_mod.in");

    assert!(dir.join("moon.mod.json").exists());

    check(
        get_stderr(&dir, ["fmt", "--dry-run", "--sort-input"]),
        expect![[r#"
            Warning: Migrating to moon.mod at module root '$ROOT', deprecated moon.mod.json is removed.
        "#]],
    );

    let output = get_stdout(&dir, ["fmt", "--dry-run", "--sort-input"]);

    check(
        output,
        expect![[r#"
            moonfmt ./main/moon.pkg -w -o ./_build/wasm-gc/release/format/main/moon.pkg
            moon tool migrate-manifest --old ./moon.mod.json --dest ./moon.mod
            moonfmt ./main/main.mbt -w -o ./_build/wasm-gc/release/format/main/main.mbt
        "#]],
    );
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

readme = "README.md"
"#
    );
}

#[test]
fn test_fmt_moon_mod_json_migration_removes_legacy_file() {
    let dir = TestDir::new("fmt_moon_mod.in");

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["fmt"])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(dir.join("moon.mod")).unwrap(),
        r#"name = "test/fmt_moon_mod_json"

version = "0.0.1"

import {
  "example/dep@0.1.0",
}

readme = "README.md"

warnings = "+w1-w2""#
    );
    assert!(!dir.join("moon.mod.json").exists());
}

#[test]
fn test_fmt_legacy_module_json_preserves_imports_and_rules() {
    let dir = TestDir::new("fmt_moon_mod.in");
    let source = r#"{
  "name": "test/legacy_module",
  "deps": {"example/dep": "0.1.0"},
  "warn-list": "+w1-w2",
  "supported-targets": "wasm-gc",
  "rule": [
    {"name": "first", "command": "echo first"},
    {"name": "second", "command": "echo second"}
  ]
}"#;
    std::fs::write(dir.join("moon.mod.json"), source).unwrap();

    moon_cmd(&dir).args(["fmt", "--check"]).assert().failure();
    moon_cmd(&dir).args(["fmt", "--warn"]).assert().success();
    assert!(!dir.join("moon.mod").exists());
    assert_eq!(
        std::fs::read_to_string(dir.join("moon.mod.json")).unwrap(),
        source
    );

    moon_cmd(&dir).arg("fmt").assert().success();
    assert!(!dir.join("moon.mod.json").exists());
    let module = moonutil::manifest::read_module_desc_file_in_dir(dir.as_ref()).unwrap();
    assert_eq!(
        module.deps["example/dep"].version().unwrap().to_string(),
        "0.1.0"
    );
    assert_eq!(module.warn_list.as_deref(), Some("+w1-w2"));
    let rules = module.rule.unwrap();
    assert_eq!(
        rules
            .iter()
            .map(|rule| (rule.name.as_str(), rule.command.as_str()))
            .collect::<Vec<_>>(),
        [("first", "echo first"), ("second", "echo second")]
    );
}
