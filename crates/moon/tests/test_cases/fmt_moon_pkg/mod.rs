use super::*;

/// Test that moon fmt with rr_moon_pkg feature gate:
/// - Migrates moon.pkg.json to moon.pkg (lib/)
/// - Formats existing moon.pkg files (main/)
#[test]
fn test_fmt_moon_pkg_json_migration_dry_run() {
    let dir = TestDir::new("fmt_moon_pkg.in");

    // Verify moon.pkg.json exists in lib/
    assert!(dir.join("lib").join("moon.pkg.json").exists());
    // Verify moon.pkg exists in main/
    assert!(dir.join("main").join("moon.pkg").exists());

    // Check stderr for migration warning
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
        expect![[r#"
            Warning: Migrating to moon.mod at module root '$ROOT', deprecated moon.mod.json is removed.
            Warning: Migrating to moon.pkg in package 'test/fmt_moon_pkg/lib', deprecated moon.pkg.json is removed.
        "#]],
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

    check(
        output,
        expect![[r#"
            moon tool migrate-manifest --old ./lib/moon.pkg.json --dest ./lib/moon.pkg
            moonfmt ./main/moon.pkg -w -o ./_build/wasm-gc/release/format/main/moon.pkg
            moon tool migrate-manifest --old ./moon.mod.json --dest ./moon.mod
            moonfmt ./main/main.mbt -w -o ./_build/wasm-gc/release/format/main/main.mbt
            moonfmt ./lib/hello.mbt -w -o ./_build/wasm-gc/release/format/lib/hello.mbt
        "#]],
    );
}

#[test]
fn test_fmt_moon_pkg_json_migration_replaces_legacy_file() {
    let dir = TestDir::new("fmt_moon_pkg.in");

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["--unstable-feature", "rr_moon_pkg", "fmt"])
        .assert()
        .success();

    assert!(dir.join("lib/moon.pkg").exists());
    assert!(!dir.join("lib/moon.pkg.json").exists());
}

#[test]
fn test_reading_package_warns_when_moon_pkg_shadows_json() {
    let dir = TestDir::new("fmt_moon_pkg_both.in");
    let stderr = get_stderr(&dir, ["check", "--dry-run"]);

    assert!(
        stderr.contains("Both moon.pkg.json and moon.pkg exist at package root"),
        "{stderr}"
    );
}

/// Test that with rr_moon_pkg and rr_moon_mod disabled, legacy manifests are not migrated,
/// but existing new-format manifests are still formatted.
#[test]
fn test_fmt_without_moon_pkg_and_moon_mod_feature() {
    let dir = TestDir::new("fmt_moon_pkg.in");

    // Test dry run output without rr_moon_pkg and rr_moon_mod features.
    check(
        get_stdout_with_envs(
            &dir,
            ["fmt", "--dry-run", "--sort-input"],
            [("NEW_MOON_PKG", "0"), ("NEW_MOON_MOD", "0")],
        ),
        expect![[r#"
            moonfmt ./main/moon.pkg -w -o ./_build/wasm-gc/release/format/main/moon.pkg
            moonfmt ./main/main.mbt -w -o ./_build/wasm-gc/release/format/main/main.mbt
            moonfmt ./lib/hello.mbt -w -o ./_build/wasm-gc/release/format/lib/hello.mbt
        "#]],
    );
}

/// Test that when both moon.pkg and moon.pkg.json exist in the same package,
/// a warning is shown and formatting proceeds with moon.pkg (new format)
#[test]
fn test_fmt_moon_pkg_both_exist() {
    let dir = TestDir::new("fmt_moon_pkg_both.in");

    // Verify both files exist in the both/ package
    assert!(dir.join("both").join("moon.pkg.json").exists());
    assert!(dir.join("both").join("moon.pkg").exists());

    // Test dry run output - should show warnings on stderr:
    // 1. Migration warning for root package (moon.pkg.json only)
    // 2. Both-exist warning for both/ package
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
        expect![[r#"
            Warning: Both moon.pkg.json and moon.pkg exist at package root '$ROOT/both', using the new format moon.pkg. Please remove the deprecated moon.pkg.json.
            Warning: Migrating to moon.mod at module root '$ROOT', deprecated moon.mod.json is removed.
            Warning: Migrating to moon.pkg in package 'test/fmt_moon_pkg_both', deprecated moon.pkg.json is removed.
        "#]],
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

    // Stdout should still show the formatting commands (using moon.pkg for both/, migrating for root)
    check(
        output,
        expect![[r#"
            moon tool migrate-manifest --old ./moon.pkg.json --dest ./moon.pkg
            moonfmt ./both/moon.pkg -w -o ./_build/wasm-gc/release/format/both/moon.pkg
            moon tool migrate-manifest --old ./moon.mod.json --dest ./moon.mod
            moonfmt ./both/lib.mbt -w -o ./_build/wasm-gc/release/format/both/lib.mbt
        "#]],
    );
}
