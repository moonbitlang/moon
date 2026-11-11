use crate::{Fixture, moon_cmd, test_case_path};

#[test]
fn test_moon_test_with_cfg() {
    let dir = Fixture::readonly(test_case_path("moon_test/with_cfg"));
    moon_cmd(&dir)
        .args(["test", "--target=wasm"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
I am always executed
I only execute on wasm
Total tests: 2, passed: 2, failed: 0.

"#]]);

    moon_cmd(&dir)
        .args(["test", "--target=js"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
I am always executed
I only execute on not wasm
Total tests: 2, passed: 2, failed: 0.

"#]]);
}
