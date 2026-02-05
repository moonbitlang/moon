use super::*;

#[test]
fn test_js_test_build_only() {
    let dir = TestDir::new("js_test_build_only/js_test_build_only.in");
    let stdout = get_stdout(&dir, ["test", "--target", "js", "--build-only"]);
    check(
        &stdout,
        expect![[r#"
            {"artifacts_path":["$ROOT/_build/js/debug/test/js_test_build_only.internal_test.cjs"]}
        "#]],
    );

    let stdout = get_stdout(&dir, ["run", "main", "--target", "js", "--build-only"]);
    check(
        &stdout,
        expect![[r#"
            {"artifacts_path":["$ROOT/_build/js/debug/build/main/main.js"]}
        "#]],
    )
}
