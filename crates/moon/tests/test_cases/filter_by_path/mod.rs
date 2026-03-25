use super::*;
use expect_test::expect_file;
use std::{ffi::OsString, path::PathBuf};

fn assert_contains_and_absent(output: &str, present: &[&str], absent: &[&str]) {
    for needle in present {
        assert!(
            output.contains(needle),
            "expected output to contain `{needle}`, got:\n{output}"
        );
    }
    for needle in absent {
        assert!(
            !output.contains(needle),
            "expected output to not contain `{needle}`, got:\n{output}"
        );
    }
}

fn create_outside_work_context_dir(dir: &TestDir, name: &str) -> PathBuf {
    let outside = dir.as_ref().parent().unwrap().join(name);
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("README.txt"), "outside work context").unwrap();
    outside
}

fn create_other_project_pkg(dir: &TestDir, name: &str) -> PathBuf {
    let root = dir.as_ref().parent().unwrap().join(name);
    let pkg = root.join("pkg");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(root.join("moon.mod.json"), "{}").unwrap();
    std::fs::write(pkg.join("moon.pkg.json"), "{}").unwrap();
    std::fs::write(pkg.join("hello.mbt"), "fn init { () }\n").unwrap();
    pkg
}

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
fn test_moon_build_filter_by_multiple_paths_success() {
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

    let stdout = get_stdout(&dir, ["build", "A", "lib", "--dry-run", "--sort-input"]);
    assert_contains_and_absent(
        &stdout,
        &["./A/hello.mbt", "./lib/hello.mbt"],
        &["./main/main.mbt"],
    );
}

#[test]
fn test_moon_build_filter_by_multiple_paths_skips_outside_current_root() {
    let dir = TestDir::new("test_filter/test_filter");
    let outside = create_outside_work_context_dir(&dir, "outside_build_skip");
    let other_pkg = create_other_project_pkg(&dir, "other_build_project");
    let outside_display = outside.display().to_string().replace('\\', "/");
    let other_pkg_display = other_pkg.display().to_string().replace('\\', "/");

    let stdout = get_stdout(
        &dir,
        [
            OsString::from("build"),
            OsString::from("A"),
            outside.as_os_str().to_os_string(),
            other_pkg.as_os_str().to_os_string(),
            OsString::from("--dry-run"),
            OsString::from("--sort-input"),
        ],
    );
    assert_contains_and_absent(
        &stdout,
        &["./A/hello.mbt"],
        &["./lib/hello.mbt", "./main/main.mbt"],
    );

    let stderr = get_stderr(
        &dir,
        [
            OsString::from("build"),
            OsString::from("A"),
            outside.as_os_str().to_os_string(),
            other_pkg.as_os_str().to_os_string(),
            OsString::from("--dry-run"),
            OsString::from("--sort-input"),
            OsString::from("--verbose"),
        ],
    );
    assert!(
        stderr.contains(&format!("skipping path `{outside_display}`")),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains(&format!("skipping path `{other_pkg_display}`")),
        "stderr: {stderr}"
    );

    let stderr = get_stderr(
        &dir,
        [
            OsString::from("build"),
            OsString::from("A"),
            outside.as_os_str().to_os_string(),
            other_pkg.as_os_str().to_os_string(),
            OsString::from("--dry-run"),
            OsString::from("--sort-input"),
        ],
    );
    assert!(!stderr.contains("skipping path"), "stderr: {stderr}");
}

#[test]
fn test_moon_build_filter_by_multiple_paths_skips_same_root_non_packages() {
    let dir = TestDir::new("test_filter/test_filter");

    let stdout = get_stdout(&dir, ["build", "A", "notes", "--dry-run", "--sort-input"]);
    assert_contains_and_absent(
        &stdout,
        &["./A/hello.mbt"],
        &["./lib/hello.mbt", "./main/main.mbt"],
    );

    let stderr = get_stderr(&dir, ["build", "A", "notes", "--dry-run", "--verbose"]);
    assert!(stderr.contains("skipping path `notes`"), "stderr: {stderr}");
}

#[test]
fn test_moon_build_filter_by_multiple_paths_skips_pkg_like_dirs_outside_source() {
    let dir = TestDir::new("path_outside_source.in");

    let stdout = get_stdout(
        &dir,
        [
            "build",
            "src/main",
            "generated/ghost",
            "--dry-run",
            "--sort-input",
        ],
    );
    assert!(stdout.contains("./src/main/main.mbt"), "stdout: {stdout}");

    let stderr = get_stderr(
        &dir,
        [
            "build",
            "src/main",
            "generated/ghost",
            "--dry-run",
            "--verbose",
        ],
    );
    assert!(
        stderr.contains("skipping path `generated/ghost`"),
        "stderr: {stderr}"
    );
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
fn test_moon_check_filter_by_multiple_paths_success() {
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

    let stdout = get_stdout(&dir, ["check", "A", "lib", "--dry-run", "--sort-input"]);
    assert_contains_and_absent(
        &stdout,
        &["./A/hello.mbt", "./lib/hello.mbt"],
        &["./main/main.mbt"],
    );
}

#[test]
fn test_moon_check_filter_by_multiple_paths_skips_outside_current_root() {
    let dir = TestDir::new("test_filter/test_filter");
    let outside = create_outside_work_context_dir(&dir, "outside_check_skip");
    let other_pkg = create_other_project_pkg(&dir, "other_check_project");
    let outside_display = outside.display().to_string().replace('\\', "/");
    let other_pkg_display = other_pkg.display().to_string().replace('\\', "/");

    let stdout = get_stdout(
        &dir,
        [
            OsString::from("check"),
            OsString::from("A"),
            outside.as_os_str().to_os_string(),
            other_pkg.as_os_str().to_os_string(),
            OsString::from("--dry-run"),
            OsString::from("--sort-input"),
        ],
    );
    assert_contains_and_absent(
        &stdout,
        &["./A/hello.mbt"],
        &["./lib/hello.mbt", "./main/main.mbt"],
    );

    let stderr = get_stderr(
        &dir,
        [
            OsString::from("check"),
            OsString::from("A"),
            outside.as_os_str().to_os_string(),
            other_pkg.as_os_str().to_os_string(),
            OsString::from("--dry-run"),
            OsString::from("--sort-input"),
            OsString::from("--verbose"),
        ],
    );
    assert!(
        stderr.contains(&format!("skipping path `{outside_display}`")),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains(&format!("skipping path `{other_pkg_display}`")),
        "stderr: {stderr}"
    );

    let stderr = get_stderr(
        &dir,
        [
            OsString::from("check"),
            OsString::from("A"),
            outside.as_os_str().to_os_string(),
            other_pkg.as_os_str().to_os_string(),
            OsString::from("--dry-run"),
            OsString::from("--sort-input"),
        ],
    );
    assert!(!stderr.contains("skipping path"), "stderr: {stderr}");
}

#[test]
fn test_moon_check_filter_by_multiple_paths_skips_same_root_non_packages() {
    let dir = TestDir::new("test_filter/test_filter");

    let stdout = get_stdout(&dir, ["check", "A", "notes", "--dry-run", "--sort-input"]);
    assert_contains_and_absent(
        &stdout,
        &["./A/hello.mbt"],
        &["./lib/hello.mbt", "./main/main.mbt"],
    );

    let stderr = get_stderr(&dir, ["check", "A", "notes", "--dry-run", "--verbose"]);
    assert!(stderr.contains("skipping path `notes`"), "stderr: {stderr}");
}

#[test]
fn test_moon_check_filter_by_multiple_paths_skips_pkg_like_dirs_outside_source() {
    let dir = TestDir::new("path_outside_source.in");

    let stdout = get_stdout(
        &dir,
        [
            "check",
            "src/main",
            "generated/ghost",
            "--dry-run",
            "--sort-input",
        ],
    );
    assert!(stdout.contains("./src/main/main.mbt"), "stdout: {stdout}");

    let stderr = get_stderr(
        &dir,
        [
            "check",
            "src/main",
            "generated/ghost",
            "--dry-run",
            "--verbose",
        ],
    );
    assert!(
        stderr.contains("skipping path `generated/ghost`"),
        "stderr: {stderr}"
    );
}

// ===== moon test command tests =====

#[test]
fn test_moon_test_filter_by_multiple_paths_success() {
    let dir = TestDir::new("test_filter/test_filter");

    let stdout = get_stdout(
        &dir,
        [
            "test",
            "A",
            "lib",
            "--dry-run",
            "--sort-input",
            "--no-parallelize",
        ],
    );
    assert_contains_and_absent(
        &stdout,
        &["./A/hello.mbt", "./lib/hello.mbt"],
        &["./main/main.mbt"],
    );
}

#[test]
fn test_moon_test_filter_by_multiple_paths_skips_same_root_non_packages() {
    let dir = TestDir::new("test_filter/test_filter");

    let stdout = get_stdout(
        &dir,
        [
            "test",
            "A",
            "notes",
            "--dry-run",
            "--sort-input",
            "--no-parallelize",
        ],
    );
    assert_contains_and_absent(
        &stdout,
        &["./A/hello.mbt"],
        &["./lib/hello.mbt", "./main/main.mbt"],
    );

    let stderr = get_stderr(
        &dir,
        [
            "test",
            "A",
            "notes",
            "--dry-run",
            "--sort-input",
            "--no-parallelize",
            "--verbose",
        ],
    );
    assert!(stderr.contains("skipping path `notes`"), "stderr: {stderr}");
}

#[test]
fn test_moon_test_filter_by_multiple_paths_skips_pkg_like_dirs_outside_source() {
    let dir = TestDir::new("path_outside_source.in");

    let stdout = get_stdout(
        &dir,
        [
            "test",
            "src/main",
            "generated/ghost",
            "--dry-run",
            "--sort-input",
            "--no-parallelize",
        ],
    );
    assert!(stdout.contains("./src/main/main.mbt"), "stdout: {stdout}");

    let stderr = get_stderr(
        &dir,
        [
            "test",
            "src/main",
            "generated/ghost",
            "--dry-run",
            "--sort-input",
            "--no-parallelize",
            "--verbose",
        ],
    );
    assert!(
        stderr.contains("skipping path `generated/ghost`"),
        "stderr: {stderr}"
    );
}
