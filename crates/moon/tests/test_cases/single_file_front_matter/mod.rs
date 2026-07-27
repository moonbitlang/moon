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
    thread::sleep(Duration::from_millis(20));
    let script = dir.join("import_ok.mbtx");
    let mut source = fs::read_to_string(&script).expect("script fixture should be readable");
    source.push('\n');
    fs::write(&script, source).expect("script fixture should be writable");

    let stdout = get_stdout(&dir, args);
    assert!(stdout.contains("hello"));
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
