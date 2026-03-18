use expect_test::expect;

use crate::{TestDir, get_stdout, util::check};

#[test]
fn test_moon_test_release() {
    let dir = TestDir::new("test_release");

    check(
        get_stdout(
            &dir,
            ["test", "--release", "--sort-input", "--no-parallelize"],
        ),
        expect![[r#"
            test A
            test hello_0
            test hello_1
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );
}
