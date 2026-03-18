use super::*;

#[test]
fn test_warn_on_dependencies_to_main_packages() {
    let dir = TestDir::new("main_package_release_n/import_main_pkg.in");
    let stderr = get_stderr(&dir, ["check", "--dry-run"]);

    assert!(
        stderr.contains("depends on main package `username/hello/main` via `import`"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("depends on main package `username/hello/main` via `wbtest-import`"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("depends on main package `username/hello/main` via `test-import`"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("package directory \"$ROOT/lib\""),
        "stderr: {stderr}"
    );
}

#[test]
fn test_warn_on_main_package_blackbox_inputs() {
    let dir = TestDir::new("main_package_release_n/main_blackbox_inputs.in");
    let stderr = get_stderr(&dir, ["check", "--dry-run"]);

    assert!(
        stderr.contains("Main package `username/hello/main` uses blackbox-only test inputs"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("`_test.mbt` files"), "stderr: {stderr}");
    assert!(stderr.contains("`.mbt.md` files"), "stderr: {stderr}");
    assert!(
        stderr.contains("package directory \"$ROOT/main\""),
        "stderr: {stderr}"
    );
}
