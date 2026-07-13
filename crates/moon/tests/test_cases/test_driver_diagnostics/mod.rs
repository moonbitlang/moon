use crate::{TestDir, moon_cmd};

#[test]
fn rendered_generated_driver_diagnostic_uses_the_physical_path() {
    let dir = TestDir::new("test_driver_diagnostics");
    moon_cmd(&dir)
        .args([
            "test",
            "--target",
            "wasm-gc",
            "--target-dir",
            "fancy-target",
        ])
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
Error: [4051]
     ╭─[ [..]/fancy-target/wasm-gc/debug/test/__generated_driver_for_internal_test.mbt:123:13 ]
     │
 123 │ priv struct MoonBitTestDriverInternalTestMap[F](
...
"#]]);
}

#[test]
fn raw_generated_driver_diagnostic_uses_the_physical_path() {
    let dir = TestDir::new("test_driver_diagnostics");
    moon_cmd(&dir)
        .args([
            "test",
            "--target",
            "wasm-gc",
            "--target-dir",
            "raw-target",
            "--no-render",
        ])
        .assert()
        .failure()
        .stdout_eq(snapbox::str![[r#"
[..]/raw-target/wasm-gc/debug/test/__generated_driver_for_internal_test.mbt:123:13-123:45 [E4051] The type MoonBitTestDriverInternalTestMap is declared twice: it was previously defined at [..]/collision.mbt:2:1.
...
"#]])
        .stderr_eq("");
}

#[test]
fn json_generated_driver_diagnostic_uses_the_physical_path() {
    let dir = TestDir::new("test_driver_diagnostics");
    moon_cmd(&dir)
        .args([
            "test",
            "--target",
            "wasm-gc",
            "--target-dir",
            "json-target",
            "--output-json",
        ])
        .assert()
        .failure()
        .stdout_eq(snapbox::str![[r#"
{"$message_type":"diagnostic","level":"error","error_code":4051,"path":"[..]json-target[..]wasm-gc[..]debug[..]test[..]__generated_driver_for_internal_test.mbt","loc":"123:13-123:45","message":"The type MoonBitTestDriverInternalTestMap is declared twice: it was previously defined at [..]collision.mbt:2:1.","context":""}
...
"#]])
        .stderr_eq("");
}
