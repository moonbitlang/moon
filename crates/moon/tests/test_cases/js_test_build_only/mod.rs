use super::*;

#[test]
fn test_js_test_build_only() {
    let dir = TestDir::new("js_test_build_only/js_test_build_only.in");
    let stdout = get_stdout(&dir, ["test", "--target", "js", "--build-only"]);
    check(
        &stdout,
        expect![[r#"
            {"artifacts_path":["$ROOT/_build/js/debug/test/js_test_build_only.blackbox_test.js","$ROOT/_build/js/debug/test/__generated_driver_for_blackbox_test.mbt","$ROOT/_build/js/debug/test/__blackbox_test_info.json","$ROOT/_build/js/debug/test/js_test_build_only.internal_test.js","$ROOT/_build/js/debug/test/__generated_driver_for_internal_test.mbt","$ROOT/_build/js/debug/test/__internal_test_info.json"]}
        "#]],
    )
}
