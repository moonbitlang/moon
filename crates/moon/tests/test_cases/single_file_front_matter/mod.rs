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
    assert!(dir.join(".mooncakes/moonbitlang/x").is_dir());
}

#[test]
fn test_single_file_mbtx_run_uses_embedded_yaml_policy() {
    let dir = TestDir::new("moon_test_single_file.in");

    moon_cmd(&dir)
        .args(["run", "embedded_policy.mbtx", "--target", "wasm"])
        .assert()
        .success()
        .stdout_eq("embedded\n");
}

#[test]
fn test_single_file_mbtx_dry_run_forwards_embedded_policy() {
    let dir = TestDir::new("moon_test_single_file.in");
    let stdout = get_stdout(
        &dir,
        [
            "run",
            "embedded_policy.mbtx",
            "--target",
            "wasm",
            "--dry-run",
        ],
    );

    assert!(
        stdout.lines().last().is_some_and(|line| {
            line.contains("--policy") && line.contains("embedded_policy.mbtx")
        }),
        "expected the script itself to be forwarded as the policy source:\n{stdout}"
    );
}

#[test]
fn test_stdin_mbtx_uses_embedded_yaml_policy() {
    let fixture_dir = TestDir::new("moon_test_single_file.in");
    let dir = TestDir::new_empty();
    let source = fs::read_to_string(fixture_dir.join("embedded_policy.mbtx")).unwrap();

    moon_cmd(&dir)
        .args(["run", "-", "--target", "wasm"])
        .stdin(source)
        .assert()
        .success()
        .stdout_eq("embedded\n");
}

#[test]
fn test_stdin_mbtx_policy_keeps_the_invocation_directory() {
    let fixture_dir = TestDir::new("moon_test_single_file.in");
    let dir = TestDir::new_empty();
    let source = fs::read_to_string(fixture_dir.join("embedded_policy.mbtx")).unwrap();
    let output = moon_cmd(&dir)
        .args(["run", "-", "--target", "wasm", "--dry-run"])
        .stdin(source)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(output).unwrap();

    assert!(
        stdout.lines().last().is_some_and(|line| {
            line.contains("--policy")
                && line.contains("stdin.mbtx")
                && line.contains("--policy-source-dir")
        }),
        "expected stdin to preserve the logical policy source directory:\n{stdout}"
    );
}

#[test]
fn test_single_file_mbtx_explicit_policy_overrides_embedded_policy() {
    let dir = TestDir::new("moon_test_single_file.in");
    fs::write(
        dir.join("explicit-policy.json"),
        r#"{"env":{"set":{"MBTX_POLICY":"explicit"}}}"#,
    )
    .unwrap();

    moon_cmd(&dir)
        .args([
            "run",
            "embedded_policy.mbtx",
            "--target",
            "wasm",
            "--wasm-policy",
            "explicit-policy.json",
        ])
        .assert()
        .success()
        .stdout_eq("explicit\n");
}

#[test]
fn test_single_file_mbtx_explicit_policy_skips_malformed_embedded_policy() {
    let dir = TestDir::new("moon_test_single_file.in");
    fs::write(
        dir.join("explicit-policy.json"),
        r#"{"env":{"set":{"MBTX_POLICY":"explicit"}}}"#,
    )
    .unwrap();

    moon_cmd(&dir)
        .args([
            "run",
            "malformed_embedded_policy.mbtx",
            "--target",
            "wasm",
            "--wasm-policy",
            "explicit-policy.json",
        ])
        .assert()
        .success()
        .stdout_eq("explicit\n");
}

#[test]
fn test_single_file_mbtx_non_wasm_paths_skip_malformed_embedded_policy() {
    let dir = TestDir::new("moon_test_single_file.in");

    for target in ["js", "native"] {
        moon_cmd(&dir)
            .args([
                "run",
                "malformed_embedded_policy.mbtx",
                "--target",
                target,
                "--dry-run",
            ])
            .assert()
            .success();
    }
}

#[test]
fn test_single_file_mbtx_build_only_skips_malformed_embedded_policy() {
    let dir = TestDir::new("moon_test_single_file.in");

    moon_cmd(&dir)
        .args([
            "run",
            "malformed_embedded_policy.mbtx",
            "--target",
            "wasm",
            "--build-only",
        ])
        .assert()
        .success();
}

#[test]
fn test_single_file_mbtx_rejects_effective_malformed_embedded_policy() {
    let dir = TestDir::new("moon_test_single_file.in");

    moon_cmd(&dir)
        .args(["run", "malformed_embedded_policy.mbtx", "--target", "wasm"])
        .assert()
        .failure();
}

#[test]
fn test_single_file_mbtx_check() {
    let dir = TestDir::new("moon_test_single_file.in");
    let _ = get_stdout(&dir, ["check", "import_ok.mbtx"]);
}

#[test]
fn test_single_file_mbtx_build() {
    let dir = TestDir::new("moon_test_single_file.in");
    let stdout = get_stdout(&dir, ["build", "moonx_args.mbtx", "--dry-run"]);

    assert!(stdout.contains("moonx_args.mbtx"), "stdout: {stdout}");
    assert!(
        stdout.contains("-ignore-import-declaration"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("moonc link-core"), "stdout: {stdout}");
    assert!(stdout.contains("_build/wasm/"), "stdout: {stdout}");
    assert!(!stdout.contains("moonrun"), "stdout: {stdout}");
}

#[test]
fn test_single_file_mbtx_build_accepts_multiple_targets() {
    let dir = TestDir::new("moon_test_single_file.in");
    let _ = get_stdout(&dir, ["build", "moonx_args.mbtx", "--target", "wasm,js"]);

    assert!(
        dir.join("_build/wasm/debug/build/single/single.wasm")
            .is_file()
    );
    assert!(dir.join("_build/js/debug/build/single/single.js").is_file());
}

#[test]
fn test_project_mbtx_builds_standalone_input() {
    let dir = TestDir::new("test_filter/test_filter");
    fs::write(
        dir.join("A/script.mbtx"),
        r#"fn main {
  println("hello")
}
"#,
    )
    .unwrap();

    let stdout = get_stdout(
        &dir,
        ["build", "A/script.mbtx", "--target", "wasm", "--dry-run"],
    );

    assert!(stdout.contains("script.mbtx"), "stdout: {stdout}");
    assert!(!stdout.contains("hello.mbt"), "stdout: {stdout}");
}

#[test]
fn test_project_mbtx_check_uses_standalone_input() {
    let dir = TestDir::new("test_filter/test_filter");
    fs::write(
        dir.join("A/script.mbtx"),
        r#"fn main {
  println("hello")
}
"#,
    )
    .unwrap();

    moon_cmd(&dir)
        .args(["check", "A/script.mbtx", "--target", "wasm"])
        .assert()
        .success();

    let metadata =
        standalone_scoped_packages_json_path(dir.join("A"), "wasm", "debug", "script.mbtx");
    assert!(
        metadata.is_file(),
        "standalone check should publish file-scoped package metadata"
    );
}

#[test]
fn test_single_file_mbtx_run_does_not_warn_about_supported_targets() {
    let dir = TestDir::new("moon_test_single_file.in");
    let stderr = get_stderr(&dir, ["run", "import_ok.mbtx"]);

    assert!(
        !stderr.contains("Package `moon/test/single` does not declare `supported_targets`"),
        "stderr: {stderr}"
    );
}

#[test]
fn test_standalone_mbt_run_rejects_relative_dependency_cache() {
    let dir = TestDir::new("moon_test_single_file.in");

    moon_cmd(&dir)
        .env("MOON_DEP_CACHE", "relative")
        .args(["run", "with_main.mbt"])
        .assert()
        .failure()
        .stderr_eq(
            "Error: Failed to resolve the module dependency graph\n\nCaused by:\n    MOON_DEP_CACHE must be an absolute path or `off`\n",
        );
}

#[test]
fn test_single_file_run_inputs_share_immutable_dependency_sources() {
    let mbtx_dir = TestDir::new("moon_test_single_file.in");
    let inline_dir = TestDir::new_empty();
    let stdin_dir = TestDir::new_empty();
    let dependency_cache = tempfile::TempDir::new().unwrap();
    let source = fs::read_to_string(mbtx_dir.join("import_ok.mbtx")).unwrap();

    moon_cmd(&mbtx_dir)
        .env("MOON_DEP_CACHE", dependency_cache.path())
        .args(["run", "import_ok.mbtx"])
        .assert()
        .success()
        .stdout_eq("hello\n");

    assert!(!mbtx_dir.join(".mooncakes/moonbitlang/x").exists());
    let cached_source = dependency_cache
        .path()
        .join("v1/sources/moonbitlang/x/0.4.38");
    assert!(
        cached_source.join("moon.mod").is_file() || cached_source.join("moon.mod.json").is_file()
    );
    let checksum = fs::read_to_string(cached_source.join(".moon-source-archive-checksum")).unwrap();
    assert_eq!(checksum.len(), 64);
    assert!(checksum.bytes().all(|byte| byte.is_ascii_hexdigit()));

    moon_cmd(&inline_dir)
        .env("MOON_DEP_CACHE", dependency_cache.path())
        .args(["run", "--frozen", "-e", &source])
        .assert()
        .success()
        .stdout_eq("hello\n");
    assert!(!inline_dir.join(".mooncakes/moonbitlang/x").exists());

    moon_cmd(&stdin_dir)
        .env("MOON_DEP_CACHE", dependency_cache.path())
        .args(["run", "--frozen", "-"])
        .stdin(source)
        .assert()
        .success()
        .stdout_eq("hello\n");
    assert!(!stdin_dir.join(".mooncakes/moonbitlang/x").exists());
}

#[test]
fn test_single_file_mbtx_dry_run_prints_dependencies_before_script() {
    let dir = TestDir::new("moon_test_single_file.in");
    let stdout = get_stdout(
        &dir,
        ["run", "import_ok.mbtx", "--target", "wasm", "--dry-run"],
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
}

#[test]
fn test_single_file_mbtx_reuses_dependency_graph_after_script_change() {
    let dir = TestDir::new("moon_test_single_file.in");
    let args = ["run", "import_ok.mbtx", "--target", "wasm"];
    let stdout = get_stdout(&dir, args);
    assert!(stdout.contains("hello"));

    let build_dir = dir.join("_build/wasm/debug/build");
    let dependency_core = build_dir.join(".mooncakes/moonbitlang/x/stack/stack.core");
    let n2_db = dir.join("_build/.moon_db");
    assert!(dependency_core.is_file());
    assert!(n2_db.is_file());

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
fn test_single_file_mbtx_dry_run_preserves_import_all_alias() {
    let dir = TestDir::new("moon_test_single_file.in");
    let stdout = get_stdout(&dir, ["run", "import_all_alias.mbtx", "--dry-run"]);
    let command = stdout
        .lines()
        .find(|line| line.contains("-pkg moon/test/single "))
        .expect("dry-run should contain the synthetic package build");

    assert!(
        command.contains("-i .mooncakes/moonbitlang/x/stack/stack.mi:*"),
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
