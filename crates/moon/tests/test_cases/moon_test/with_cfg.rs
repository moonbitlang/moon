use expect_test::expect;

use crate::{TestDir, get_stdout, util::check};

#[test]
fn test_moon_test_with_cfg() {
    let dir = TestDir::new("moon_test/with_cfg");
    check(
        get_stdout(&dir, ["test", "--target=wasm"]),
        expect![[r#"
            I am always executed
            I only execute on wasm
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--target=js"]),
        expect![[r#"
            I am always executed
            I only execute on not wasm
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
}
