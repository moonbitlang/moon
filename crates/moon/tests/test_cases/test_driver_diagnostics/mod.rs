use crate::{TestDir, moon_cmd};

fn failed_test_output(dir: &TestDir, args: &[&str]) -> String {
    let assert = moon_cmd(dir).args(args).assert().failure();
    let output = assert.get_output();
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .replace('\\', "/")
}

fn assert_uses_physical_driver_path(output: &str, target_dir: &str) {
    let driver_path =
        format!("{target_dir}/wasm-gc/debug/test/__generated_driver_for_internal_test.mbt");
    assert!(
        output.contains(&driver_path),
        "diagnostic should use the generated driver's physical path:\n{output}"
    );
    assert!(
        !output.contains("failed to read file"),
        "generated driver should be available to the diagnostic renderer:\n{output}"
    );
}

#[test]
fn generated_driver_diagnostics_use_the_physical_path() {
    let dir = TestDir::new("test_driver_diagnostics");
    let fancy = failed_test_output(
        &dir,
        &[
            "test",
            "--target",
            "wasm-gc",
            "--target-dir",
            "fancy-target",
        ],
    );
    assert_uses_physical_driver_path(&fancy, "fancy-target");

    let raw = failed_test_output(
        &dir,
        &[
            "test",
            "--target",
            "wasm-gc",
            "--target-dir",
            "raw-target",
            "--no-render",
        ],
    );
    assert_uses_physical_driver_path(&raw, "raw-target");

    let json = failed_test_output(
        &dir,
        &[
            "test",
            "--target",
            "wasm-gc",
            "--target-dir",
            "json-target",
            "--output-json",
        ],
    );
    assert_uses_physical_driver_path(&json, "json-target");
}
