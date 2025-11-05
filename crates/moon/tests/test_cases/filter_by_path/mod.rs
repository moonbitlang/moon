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

    // Test error handling for file paths (not supported by build)
    let stdout = get_stdout(&dir, ["build", "A/hello.mbt", "--dry-run", "--sort-input"]);
    expect_file!["snapshots/build_A_file.stdout"].assert_eq(&stdout);
}

#[test]
fn test_moon_build_filter_by_path_failure() {
    let dir = TestDir::new("test_filter/test_filter");

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

    // Multiple folders should be rejected
    let _stderr = get_err_stderr(&dir, ["build", "A", "lib", "--dry-run", "--sort-input"]);
}

// ===== moon check command tests =====

#[test]
fn test_moon_check_filter_by_path_success() {
    let dir = TestDir::new("test_filter/test_filter");

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

    // Multiple folders should be rejected
    let _stderr = get_err_stderr(&dir, ["check", "A", "lib", "--dry-run", "--sort-input"]);
}
