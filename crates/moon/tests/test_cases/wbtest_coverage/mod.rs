use super::*;

#[test]
fn test_wbtest_coverage() {
    let dir = TestDir::new("wbtest_coverage/wbtest_coverage.in");

    let stdout = get_stdout(&dir, ["test", "--enable-coverage"]);
    check(
        &stdout,
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}
