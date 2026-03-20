use super::*;
use expect_test::expect_file;

fn run_info_and_clear(
    base_dir: &TestDir,
    working_dir: &impl AsRef<std::path::Path>,
    args: &[&str],
    pkg_rel_path: &str,
) {
    let pkg_path = base_dir.join(pkg_rel_path);
    let generated = pkg_path.join(MBTI_GENERATED);

    if generated.exists() {
        std::fs::remove_file(&generated).unwrap();
    }

    check(
        get_stdout(working_dir, args.iter().copied()),
        expect![[r#""#]],
    );

    assert!(
        generated.exists(),
        "moon info did not create {} for {:?}",
        MBTI_GENERATED,
        args,
    );

    let content = std::fs::read_to_string(&generated).unwrap();
    assert!(
        !content.is_empty(),
        "{} should not be empty",
        MBTI_GENERATED
    );

    std::fs::remove_file(&generated).unwrap();
}

// ===== moon info command tests =====

#[test]
fn test_moon_info_filter_by_path_success() {
    let dir = TestDir::new("test_filter/test_filter");
    // Test info with folder path
    run_info_and_clear(&dir, &dir, &["info", "A"], "A");

    // Test info with folder path with trailing slash
    run_info_and_clear(&dir, &dir, &["info", "A/"], "A");

    // Test info with file path
    run_info_and_clear(&dir, &dir, &["info", "A/hello.mbt"], "A");

    // Test info from inside package directory
    run_info_and_clear(&dir, &dir.join("A"), &["info", "."], "A");

    // Test info with lib folder
    run_info_and_clear(&dir, &dir, &["info", "lib"], "lib");

    // Test info with main folder
    run_info_and_clear(&dir, &dir, &["info", "main"], "main");

    // Test info with relative paths using dot notation
    run_info_and_clear(&dir, &dir, &["info", "./main"], "main");
}

#[test]
fn test_moon_info_filter_by_path_failure() {
    let dir = TestDir::new("test_filter/test_filter");

    // Test error handling for non-existent paths
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["info", "nonexistent"])
        .assert()
        .failure();

    // Test error handling for invalid file paths
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["info", "A/nonexistent.mbt"])
        .assert()
        .failure();

    // Test error handling for non-existent folder paths
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["info", "invalid_folder"])
        .assert()
        .failure();
}

// ===== moon run command tests =====

#[test]
fn test_moon_run_filter_by_path_success() {
    let dir = TestDir::new("test_filter/test_filter");

    // Test run with main package folder
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
        Hello, world!
    "#]],
    );

    // Test run with main package folder with trailing slash
    check(
        get_stdout(&dir, ["run", "main/"]),
        expect![[r#"
        Hello, world!
    "#]],
    );

    // Test run with main file path
    check(
        get_stdout(&dir, ["run", "main/main.mbt"]),
        expect![[r#"
        Hello, world!
    "#]],
    );

    // Test run with relative paths using dot notation
    check(
        get_stdout(&dir, ["run", "./main"]),
        expect![[r#"
        Hello, world!
    "#]],
    );

    // FIXME: `moon run` paths are based on project root, thus cwd-based
    // paths like below are not supported yet.
    //
    // // Test run from inside package directory
    // check(
    //     get_stdout(&dir.join("main"), ["run", "."]),
    //     expect![[r#"
    //     Hello, world!
    // "#]],
    // );
}

#[test]
fn test_moon_run_filter_by_path_failure() {
    let dir = TestDir::new("test_filter/test_filter");

    // Test error handling for non-existent paths
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["run", "nonexistent"])
        .assert()
        .failure();

    // Test error handling for invalid file paths
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["run", "main/nonexistent.mbt"])
        .assert()
        .failure();

    // Test error handling for non-executable packages
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["run", "lib"])
        .assert()
        .failure();

    // Test error handling for non-existent folder paths
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["run", "invalid_folder"])
        .assert()
        .failure();
}

// ===== moon build command tests =====

#[test]
fn test_moon_build_filter_by_path_success() {
    let dir = TestDir::new("test_filter/test_filter");

    // Test build with folder path
    let stdout = get_stdout(&dir, ["build", "A", "--dry-run", "--sort-input"]);
    expect_file!["snapshots/build_A.stdout"].assert_eq(&stdout);

    // Test build with folder path with trailing slash
    let stdout = get_stdout(&dir, ["build", "A/", "--dry-run", "--sort-input"]);
    expect_file!["snapshots/build_A_slash.stdout"].assert_eq(&stdout);

    // Test build with lib folder
    let stdout = get_stdout(&dir, ["build", "lib", "--dry-run", "--sort-input"]);
    expect_file!["snapshots/build_lib.stdout"].assert_eq(&stdout);

    // Test build with main folder
    let stdout = get_stdout(&dir, ["build", "main", "--dry-run", "--sort-input"]);
    expect_file!["snapshots/build_main.stdout"].assert_eq(&stdout);

    // Test build from inside package directory
    let stdout = get_stdout(&dir.join("A"), ["build", ".", "--dry-run", "--sort-input"]);
    expect_file!["snapshots/build_A_dot.stdout"].assert_eq(&stdout);

    // Test build with relative paths using dot notation
    let stdout = get_stdout(&dir, ["build", "./lib", "--dry-run", "--sort-input"]);
    expect_file!["snapshots/build_relative_lib.stdout"].assert_eq(&stdout);

    // Test build with a file path, which resolves through the containing package
    let stdout = get_stdout(&dir, ["build", "A/hello.mbt", "--dry-run", "--sort-input"]);
    expect_file!["snapshots/build_A_file.stdout"].assert_eq(&stdout);

    // Test build with multiple paths, deduplicating repeated package matches
    let stdout = get_stdout(
        &dir,
        [
            "build",
            "A",
            "A/hello.mbt",
            "lib",
            "lib/hello.mbt",
            "--dry-run",
            "--sort-input",
        ],
    );
    expect_file!["snapshots/build_A_lib.stdout"].assert_eq(&stdout);
}

#[test]
fn test_moon_build_filter_by_path_failure() {
    let dir = TestDir::new("test_filter/test_filter");
    std::fs::write(dir.join("orphan.mbt"), "fn orphan() -> Int { 0 }\n").unwrap();

    // Test error handling for non-existent paths
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["build", "nonexistent", "--dry-run"])
        .assert()
        .failure();

    // Test error handling for invalid folder paths
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["build", "invalid_folder", "--dry-run"])
        .assert()
        .failure();

    let stderr = get_err_stderr(&dir, ["build", "orphan.mbt", "--dry-run"]);
    assert!(
        stderr.contains("does not contain `moon.pkg` or `moon.pkg.json`, so it is not a package"),
        "stderr: {stderr}"
    );

    let stdout = get_stdout(
        &dir,
        [
            "build",
            ".",
            "moon.mod.json",
            "A",
            "A/hello.mbt",
            "lib",
            "lib/hello.mbt",
            "--dry-run",
            "--sort-input",
        ],
    );
    expect_file!["snapshots/build_A_lib.stdout"].assert_eq(&stdout);

    check(
        get_stderr(
            &dir,
            [
                "build",
                ".",
                "moon.mod.json",
                "A",
                "A/hello.mbt",
                "lib",
                "lib/hello.mbt",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            Warning: skipping `.` because it does not contain `moon.pkg` or `moon.pkg.json`
            Warning: skipping `moon.mod.json` because only package directories and MoonBit source files are accepted
        "#]],
    );

    let stderr = get_err_stderr(&dir, ["build", "moon.mod.json", "--dry-run"]);
    assert!(
        stderr.contains("Warning: skipping `moon.mod.json`"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("None of the provided paths resolve to a package"),
        "stderr: {stderr}"
    );
}

// ===== moon check command tests =====

#[test]
fn test_moon_check_filter_by_path_success() {
    let dir = TestDir::new("test_filter/test_filter");
    std::fs::write(dir.join("orphan.mbt"), "fn orphan() -> Int { 0 }\n").unwrap();

    // Test check with folder path
    let stdout = get_stdout(&dir, ["check", "A", "--dry-run", "--sort-input"]);
    expect_file!["snapshots/check_A.stdout"].assert_eq(&stdout);

    // Test check with folder path with trailing slash
    let stdout = get_stdout(&dir, ["check", "A/", "--dry-run", "--sort-input"]);
    expect_file!["snapshots/check_A_slash.stdout"].assert_eq(&stdout);

    // Test check with lib folder
    let stdout = get_stdout(&dir, ["check", "lib", "--dry-run", "--sort-input"]);
    expect_file!["snapshots/check_lib.stdout"].assert_eq(&stdout);

    // Test check with main folder
    let stdout = get_stdout(&dir, ["check", "main", "--dry-run", "--sort-input"]);
    expect_file!["snapshots/check_main.stdout"].assert_eq(&stdout);

    // Test check from inside package directory
    let stdout = get_stdout(&dir.join("A"), ["check", ".", "--dry-run", "--sort-input"]);
    expect_file!["snapshots/check_A_dot.stdout"].assert_eq(&stdout);

    // Test check with file path
    let stdout = get_stdout(&dir, ["check", "A/hello.mbt", "--dry-run", "--sort-input"]);
    expect_file!["snapshots/check_A_file.stdout"].assert_eq(&stdout);

    // Test check with file path from inside package
    let stdout = get_stdout(
        &dir.join("A"),
        ["check", "hello.mbt", "--dry-run", "--sort-input"],
    );
    expect_file!["snapshots/check_A_file_inside.stdout"].assert_eq(&stdout);

    // Test check with relative paths using dot notation
    let stdout = get_stdout(&dir, ["check", "./A", "--dry-run", "--sort-input"]);
    expect_file!["snapshots/check_relative_A.stdout"].assert_eq(&stdout);

    // Test check with nested relative paths
    let stdout = get_stdout(
        &dir,
        ["check", "./A/hello.mbt", "--dry-run", "--sort-input"],
    );
    expect_file!["snapshots/check_relative_A_file.stdout"].assert_eq(&stdout);

    // Test check with multiple paths, deduplicating repeated package matches
    let stdout = get_stdout(
        &dir,
        [
            "check",
            "A",
            "A/hello.mbt",
            "lib",
            "lib/hello.mbt",
            "--dry-run",
            "--sort-input",
        ],
    );
    expect_file!["snapshots/check_A_lib.stdout"].assert_eq(&stdout);

    // Test standalone single-file check from module root without moon.pkg
    let stdout = get_stdout(&dir, ["check", "orphan.mbt", "--dry-run"]);
    assert!(stdout.contains("-single-file"), "stdout: {stdout}");
    assert!(stdout.contains("moon/test/single"), "stdout: {stdout}");
}

#[test]
fn test_moon_check_filter_by_path_failure() {
    let dir = TestDir::new("test_filter/test_filter");

    // Test error handling for non-existent paths
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check", "nonexistent", "--dry-run"])
        .assert()
        .failure();

    // Test error handling for invalid file paths
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check", "A/nonexistent.mbt", "--dry-run"])
        .assert()
        .failure();

    // Test error handling for invalid folder paths
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check", "invalid_folder", "--dry-run"])
        .assert()
        .failure();

    let stdout = get_stdout(
        &dir,
        [
            "check",
            ".",
            "moon.mod.json",
            "A",
            "A/hello.mbt",
            "lib",
            "lib/hello.mbt",
            "--dry-run",
            "--sort-input",
        ],
    );
    expect_file!["snapshots/check_A_lib.stdout"].assert_eq(&stdout);

    check(
        get_stderr(
            &dir,
            [
                "check",
                ".",
                "moon.mod.json",
                "A",
                "A/hello.mbt",
                "lib",
                "lib/hello.mbt",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            Warning: skipping `.` because it does not contain `moon.pkg` or `moon.pkg.json`
            Warning: skipping `moon.mod.json` because only package directories and MoonBit source files are accepted
        "#]],
    );

    let stderr = get_err_stderr(&dir, ["check", "moon.mod.json", "--dry-run"]);
    assert!(
        stderr.contains("Warning: skipping `moon.mod.json`"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("None of the provided paths resolve to a package"),
        "stderr: {stderr}"
    );

    let stderr = get_err_stderr(&dir, ["check", "A", "lib", "--dry-run", "--no-mi"]);
    assert!(
        stderr
            .contains("`--no-mi` can only be used when the selector resolves to a single package"),
        "stderr: {stderr}"
    );
}

#[test]
fn test_moon_build_and_check_across_nested_module_roots() {
    let dir = TestDir::new("indirect_dep.in");

    let build_stderr = get_stderr(
        &dir,
        [
            "build",
            ".",
            "indirect_dep1/sub",
            "indirect_dep2/sub",
            "--sort-input",
        ],
    );
    assert!(
        build_stderr.contains(
            "Warning: skipping `.` because it is not inside any Moon module or workspace"
        ),
        "stderr: {build_stderr}"
    );
    assert!(
        dir.join("indirect_dep1/sub/_build/wasm-gc/debug/build/sub.core")
            .exists(),
        "expected build artifact for indirect_dep1/sub"
    );
    assert!(
        dir.join("indirect_dep2/sub/_build/wasm-gc/debug/build/sub.core")
            .exists(),
        "expected build artifact for indirect_dep2/sub"
    );

    let check_stderr = get_stderr(
        &dir,
        [
            "check",
            ".",
            "indirect_dep1/sub",
            "indirect_dep2/sub",
            "--sort-input",
        ],
    );
    assert!(
        check_stderr.contains(
            "Warning: skipping `.` because it is not inside any Moon module or workspace"
        ),
        "stderr: {check_stderr}"
    );
    assert!(
        dir.join("indirect_dep1/sub/_build/wasm-gc/debug/check/sub.mi")
            .exists(),
        "expected check artifact for indirect_dep1/sub"
    );
    assert!(
        dir.join("indirect_dep2/sub/_build/wasm-gc/debug/check/sub.mi")
            .exists(),
        "expected check artifact for indirect_dep2/sub"
    );
}
