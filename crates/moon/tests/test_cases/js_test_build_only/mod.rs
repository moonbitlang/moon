use super::*;

#[test]
fn test_js_test_build_only() {
    let dir = TestDir::new("js_test_build_only/js_test_build_only.in");
    let stdout = get_stdout(&dir, ["test", "--target", "js", "--build-only"]);
    check(
        &stdout,
        expect![[r#"
            {"artifacts_path":["$ROOT/_build/js/debug/test/js_test_build_only.internal_test.js"],"test_filter_args":["{/"package/":/"js_test_build_only/",/"file_and_index/":[[/"src.mbt/",[{/"start/":0,/"end/":1}]],[/"src.mbt/",[]],[/"src.mbt/",[]],[/"src.mbt/",[]]]}"]}
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

#[test]
fn test_js_test_build_only_dry_run_does_not_print_artifact_json() {
    let dir = TestDir::new("js_test_build_only/js_test_build_only.in");
    let stdout = get_stdout(
        &dir,
        ["test", "--target", "js", "--build-only", "--dry-run"],
    );

    assert!(
        !stdout.contains("\"artifacts_path\""),
        "stdout should not contain build-only artifact json in dry-run mode:\n{stdout}"
    );
    assert!(
        stdout.contains("moon generate-test-driver"),
        "stdout should contain the dry-run plan:\n{stdout}"
    );
}
