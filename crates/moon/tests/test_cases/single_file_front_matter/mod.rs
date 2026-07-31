use super::*;
use std::{fs, thread, time::Duration};

#[test]
fn test_single_file_front_matter_import_ok() {
    let dir = TestDir::new("moon_test_single_file.in");
    let _ = get_stdout(&dir, ["check", "front_matter_import_ok.mbt.md"]);
}

#[test]
fn test_single_file_front_matter_import_missing_dep() {
    let dir = TestDir::new("moon_test_single_file.in");
    let stderr = get_err_stderr(&dir, ["check", "front_matter_import_missing_dep.mbt.md"]);
    assert!(stderr.contains("module 'moonbitlang/x' must include a version in moonbit.import"));
}

#[test]
fn test_single_file_front_matter_import_replaces_import_all() {
    let dir = TestDir::new("moon_test_single_file.in");
    let stdout = get_stdout(
        &dir,
        [
            "check",
            "front_matter_import_missing_pkg.mbt.md",
            "--dry-run",
        ],
    );
    assert!(stdout.contains("stack/stack.mi:xstack"));
    assert!(!stdout.contains("crypto/crypto.mi"));
}

#[test]
fn test_single_file_front_matter_deps_only_keeps_legacy_import_all_with_warning() {
    let dir = TestDir::new("moon_test_single_file.in");
    let _ = get_stdout(
        &dir,
        ["check", "front_matter_deps_only.mbt.md", "--dry-run"],
    );

    let stderr = get_stderr(
        &dir,
        ["check", "front_matter_deps_only.mbt.md", "--dry-run"],
    );
    assert!(
        stderr.contains(
            "moonbit.deps without moonbit.import: importing all packages (legacy behavior)."
        ),
        "stderr: {stderr}"
    );
}

#[test]
fn test_single_file_front_matter_import_module_root() {
    let dir = TestDir::new("moon_test_single_file.in");
    let stdout = get_stdout(&dir, ["test", "t.mbt.md", "--no-parallelize"]);
    assert!(stdout.contains("Total tests: 2, passed: 2, failed: 0."));
}

#[test]
fn test_single_file_mbtx_run() {
    let dir = TestDir::new("moon_test_single_file.in");
    let stdout = get_stdout(&dir, ["run", "import_ok.mbtx"]);
    assert!(stdout.contains("hello"));
}

#[test]
fn test_single_file_mbtx_dry_run_prints_dependencies_before_script() {
    let dir = TestDir::new("moon_test_single_file.in");
    let cache_parent = tempfile::TempDir::new().unwrap();
    let build_cache = cache_parent.path().join("build-cache");
    let stdout = get_stdout_with_envs(
        &dir,
        ["run", "import_ok.mbtx", "--target", "wasm", "--dry-run"],
        [("MOON_BUILD_CACHE", build_cache.as_os_str())],
    );
    let dependency_command = stdout
        .lines()
        .position(|line| line.contains(".mooncakes/moonbitlang/x/stack/stack.mbt"))
        .expect("dry run should print the dependency package command");
    let script_command = stdout
        .lines()
        .position(|line| line.contains("-pkg moon/test/single "))
        .expect("dry run should print the script package command");

    assert!(
        dependency_command < script_command,
        "dependency commands should be printed before script commands:\n{stdout}"
    );
    assert!(
        !build_cache.exists(),
        "dry run should not initialize or write the build cache"
    );
}

#[test]
fn test_single_file_mbtx_reuses_dependency_graph_after_script_change() {
    let dir = TestDir::new("moon_test_single_file.in");
    let args = ["run", "import_ok.mbtx", "--target", "wasm"];
    let stdout = get_stdout(&dir, args);
    assert!(stdout.contains("hello"));

    let build_dir = dir.join("_build/wasm/debug/build");
    let dependency_core = build_dir.join(".mooncakes/moonbitlang/x/stack/stack.core");
    let dependency_db = build_dir.join("standalone-dependencies.moon_db");
    let script_db = build_dir.join("build.moon_db");
    assert!(dependency_core.is_file());
    assert!(dependency_db.is_file());
    assert!(script_db.is_file());

    let dependency_modified = fs::metadata(&dependency_core)
        .expect("dependency artifact should have metadata")
        .modified()
        .expect("dependency artifact should have a modification time");
    thread::sleep(Duration::from_millis(100));
    let script = dir.join("import_ok.mbtx");
    let source = fs::read_to_string(&script).expect("script fixture should be readable");
    let original_output = r#"println("hello")"#;
    assert_eq!(
        source.matches(original_output).count(),
        1,
        "script fixture should contain exactly one output to replace",
    );
    let source = source.replacen(original_output, r#"println("updated script")"#, 1);
    fs::write(&script, source).expect("script fixture should be writable");

    let stdout = get_stdout(&dir, args);
    assert!(stdout.contains("updated script"));
    assert_eq!(
        fs::metadata(dependency_core)
            .expect("dependency artifact should still have metadata")
            .modified()
            .expect("dependency artifact should still have a modification time"),
        dependency_modified,
        "changing only the script should not rebuild dependency packages",
    );
}

#[test]
fn test_single_file_mbtx_restores_dependency_outputs_before_running_script_graph() {
    let dir = TestDir::new("moon_test_single_file.in");
    let build_cache = tempfile::TempDir::new().unwrap();
    let args = ["run", "import_ok.mbtx", "--target", "wasm"];

    moon_cmd(&dir)
        .env("MOON_BUILD_CACHE", build_cache.path())
        .args(args)
        .assert()
        .success()
        .stdout_eq("hello\n");
    assert_eq!(
        fs::read_to_string(build_cache.path().join(".moon-cache")).unwrap(),
        "build-artifacts\n"
    );

    let build_dir = dir.join("_build/wasm/debug/build");
    assert!(
        !build_dir.join("standalone-dependencies.moon_db").exists(),
        "cache-enabled misses should use private n2 state"
    );
    let dependency_outputs = [
        ".mooncakes/moonbitlang/x/stack/stack.mi",
        ".mooncakes/moonbitlang/x/stack/stack.core",
    ]
    .map(|output| build_dir.join(output));
    let expected_outputs = dependency_outputs
        .each_ref()
        .map(|output| fs::read(output).unwrap());
    for output in &dependency_outputs {
        fs::remove_file(output).unwrap();
    }
    fs::remove_file(build_dir.join("build.moon_db")).unwrap();

    let assert = moon_cmd(&dir)
        .env("MOON_BUILD_CACHE", build_cache.path())
        .args(["run", "import_ok.mbtx", "--target", "wasm", "--verbose"])
        .assert()
        .success()
        .stdout_eq("hello\n");
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        !stderr.contains(".mooncakes/moonbitlang/x/stack/stack.mbt"),
        "dependency command should not execute on a cache hit:\n{stderr}"
    );
    assert!(
        stderr.contains("moonrun "),
        "the script artifact should still run after dependency preparation:\n{stderr}"
    );
    for (output, expected) in dependency_outputs.iter().zip(expected_outputs) {
        assert_eq!(fs::read(output).unwrap(), expected);
    }
}

#[test]
fn test_single_file_mbtx_run_block_import() {
    let dir = TestDir::new("moon_test_single_file.in");
    let stdout = get_stdout(&dir, ["run", "import_block_ok.mbtx"]);
    assert!(stdout.contains("hello"));
}

#[test]
fn test_single_file_mbtx_builds_original_source() {
    let dir = TestDir::new("moon_test_single_file.in");
    let stdout = get_stdout(&dir, ["run", "import_ok.mbtx", "--dry-run"]);
    let command = stdout
        .lines()
        .find(|line| line.contains("-pkg moon/test/single "))
        .expect("dry-run should contain the synthetic package build");

    assert!(command.contains("import_ok.mbtx"), "command: {command}");
    assert!(
        command.contains("-ignore-import-declaration"),
        "command: {command}"
    );
}

#[test]
fn test_single_file_check_rejects_patch_file() {
    let dir = TestDir::new("moon_test_single_file.in");

    let patch_stderr = get_err_stderr(
        &dir,
        [
            "check",
            "front_matter_import_ok.mbt.md",
            "--patch-file",
            "patch.json",
        ],
    );
    assert!(
        patch_stderr.contains("does not support `--patch-file`"),
        "stderr: {patch_stderr}"
    );
}
