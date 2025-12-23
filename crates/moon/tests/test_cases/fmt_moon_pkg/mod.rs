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

    // Test dry run output with rr_moon_pkg feature
    if cfg!(windows) {
        check(
            output,
            expect![[r#"
                moonfmt ./lib/moon.pkg.json -o ./target/wasm-gc/release/format/lib/moon.pkg
                cmd /c copy ./target/wasm-gc/release/format/lib/moon.pkg ./lib/moon.pkg
                cmd /c del ./lib/moon.pkg.json
                moonfmt ./main/moon.pkg -w -o ./target/wasm-gc/release/format/main/moon.pkg
                moonfmt ./main/main.mbt -w -o ./target/wasm-gc/release/format/main/main.mbt
                moonfmt ./lib/hello.mbt -w -o ./target/wasm-gc/release/format/lib/hello.mbt
            "#]],
        );
    } else {
        check(
            output,
            expect![[r#"
                moonfmt ./lib/moon.pkg.json -o ./target/wasm-gc/release/format/lib/moon.pkg
                cp ./target/wasm-gc/release/format/lib/moon.pkg ./lib/moon.pkg
                rm ./lib/moon.pkg.json
                moonfmt ./main/moon.pkg -w -o ./target/wasm-gc/release/format/main/moon.pkg
                moonfmt ./main/main.mbt -w -o ./target/wasm-gc/release/format/main/main.mbt
                moonfmt ./lib/hello.mbt -w -o ./target/wasm-gc/release/format/lib/hello.mbt
            "#]],
        );
    }
}

/// Test that without rr_moon_pkg feature, moon.pkg files are not formatted
#[test]
fn test_fmt_without_moon_pkg_feature() {
    let dir = TestDir::new("fmt_moon_pkg.in");

    // Test dry run output without rr_moon_pkg feature
    check(
        get_stdout(&dir, ["fmt", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonfmt ./main/main.mbt -w -o ./target/wasm-gc/release/format/main/main.mbt
            moonfmt ./lib/hello.mbt -w -o ./target/wasm-gc/release/format/lib/hello.mbt
        "#]],
    );
}

/// Test that when both moon.pkg and moon.pkg.json exist in the same package,
/// an error is reported but formatting still proceeds with moon.pkg
#[test]
fn test_fmt_moon_pkg_both_exist() {
    let dir = TestDir::new("fmt_moon_pkg_both.in");

    // Verify both files exist in the both/ package
    assert!(dir.join("both").join("moon.pkg.json").exists());
    assert!(dir.join("both").join("moon.pkg").exists());

    // Test dry run output - should show error on stderr and format moon.pkg
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
            error: Both moon.pkg and moon.pkg.json exist in package test/fmt_moon_pkg_both/both. Please remove one of them.
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

    // Stdout should still show the formatting commands (using moon.pkg, not migrating)
    if cfg!(windows) {
        check(
            output,
            expect![[r#"
                moonfmt ./moon.pkg.json -o ./target/wasm-gc/release/format/moon.pkg
                cmd /c copy ./target/wasm-gc/release/format/moon.pkg ./moon.pkg
                cmd /c del ./moon.pkg.json
                moonfmt ./both/moon.pkg -w -o ./target/wasm-gc/release/format/both/moon.pkg
                moonfmt ./both/lib.mbt -w -o ./target/wasm-gc/release/format/both/lib.mbt
            "#]],
        );
    } else {
        check(
            output,
            expect![[r#"
                moonfmt ./moon.pkg.json -o ./target/wasm-gc/release/format/moon.pkg
                cp ./target/wasm-gc/release/format/moon.pkg ./moon.pkg
                rm ./moon.pkg.json
                moonfmt ./both/moon.pkg -w -o ./target/wasm-gc/release/format/both/moon.pkg
                moonfmt ./both/lib.mbt -w -o ./target/wasm-gc/release/format/both/lib.mbt
            "#]],
        );
    }
}
