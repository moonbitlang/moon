use super::*;
use expect_test::expect_file;
use moonutil::common::MBTI_GENERATED;

#[test]
fn test_moon_info_compare_backends() {
    // Run moon info against all backends; the fixture differs across backends
    let dir = TestDir::new("moon_info_compare_backends");
    let out = get_stdout(&dir, ["info", "--target", "all"]);

    // Divergent outputs should still promote the canonical interface to package dir
    assert!(
        dir.join("lib").join(MBTI_GENERATED).exists(),
        "Diverging backends should still promote the canonical mbti output"
    );
    let mbti = std::fs::read_to_string(dir.join("lib").join(MBTI_GENERATED)).unwrap();
    assert!(mbti.contains("pub fn hello_wasm_gc() -> String"));

    expect_file!["moon_info_compare_backends.out"].assert_eq(&out);
}
