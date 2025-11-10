use super::*;
use expect_test::expect_file;
use moonutil::common::MBTI_GENERATED;

#[test]
fn test_moon_info_compare_backends() {
    // Run moon info against all backends; the fixture differs across backends
    let dir = TestDir::new("moon_info_compare_backends");
    let out = get_err_stdout(&dir, ["info", "--target", "all"]);

    // Canonical target (prefer WasmGC) should be promoted to package dir
    assert!(
        dir.join("lib").join(MBTI_GENERATED).exists(),
        "Canonical backend's mbti should be promoted to lib/MBTI_GENERATED"
    );

    expect_file!["moon_info_compare_backends.out"].assert_eq(&out);
}
