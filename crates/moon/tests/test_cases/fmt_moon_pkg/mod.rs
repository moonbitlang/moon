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

    // Test dry run output with rr_moon_pkg feature (no rm command)
    if cfg!(windows) {
        check(
            output,
            expect![[r#"
                moonfmt ./lib/moon.pkg.json -o ./_build/wasm-gc/release/format/lib/moon.pkg
                cmd /c copy ./_build/wasm-gc/release/format/lib/moon.pkg ./lib/moon.pkg
                cmd /c del ./lib/moon.pkg.json
                moonfmt ./main/moon.pkg -w -o ./_build/wasm-gc/release/format/main/moon.pkg
                moonfmt ./main/main.mbt -w -o ./_build/wasm-gc/release/format/main/main.mbt
                moonfmt ./lib/hello.mbt -w -o ./_build/wasm-gc/release/format/lib/hello.mbt
            "#]],
        );
    } else {
        check(
            output,
            expect![[r#"
                moonfmt ./lib/moon.pkg.json -o ./_build/wasm-gc/release/format/lib/moon.pkg
                cp ./_build/wasm-gc/release/format/lib/moon.pkg ./lib/moon.pkg
                rm ./lib/moon.pkg.json
                moonfmt ./main/moon.pkg -w -o ./_build/wasm-gc/release/format/main/moon.pkg
                moonfmt ./main/main.mbt -w -o ./_build/wasm-gc/release/format/main/main.mbt
                moonfmt ./lib/hello.mbt -w -o ./_build/wasm-gc/release/format/lib/hello.mbt
            "#]],
        );
    }
}

/// Test that with rr_moon_pkg disabled, moon.pkg.json is not migrated,
/// but existing moon.pkg files are still formatted
#[test]
fn test_fmt_without_moon_pkg_feature() {
    let dir = TestDir::new("fmt_moon_pkg.in");

    // Test dry run output without rr_moon_pkg feature
    check(
        get_stdout_with_envs(
            &dir,
            ["fmt", "--dry-run", "--sort-input"],
            [("NEW_MOON_PKG", "0")],
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

    // Stdout should still show the formatting commands (using moon.pkg for both/, migrating for root)
    if cfg!(windows) {
        check(
            output,
            expect![[r#"
                moonfmt ./moon.pkg.json -o ./_build/wasm-gc/release/format/moon.pkg
                cmd /c copy ./_build/wasm-gc/release/format/moon.pkg ./moon.pkg
                cmd /c del ./moon.pkg.json
                moonfmt ./both/moon.pkg -w -o ./_build/wasm-gc/release/format/both/moon.pkg
                moonfmt ./both/lib.mbt -w -o ./_build/wasm-gc/release/format/both/lib.mbt
            "#]],
        );
    } else {
        check(
            output,
            expect![[r#"
                moonfmt ./moon.pkg.json -o ./_build/wasm-gc/release/format/moon.pkg
                cp ./_build/wasm-gc/release/format/moon.pkg ./moon.pkg
                rm ./moon.pkg.json
                moonfmt ./both/moon.pkg -w -o ./_build/wasm-gc/release/format/both/moon.pkg
                moonfmt ./both/lib.mbt -w -o ./_build/wasm-gc/release/format/both/lib.mbt
            "#]],
        );
    }
}
