use super::*;

#[test]
fn test_moonlex() {
    // Note: previously there's a check about installed `moonlex` binary,
    // but since it comes with the distribution now, we can skip that.
    let dir = TestDir::new("prebuild/moonlex");
    let _ = get_stdout(&dir, ["check"]);
    assert!(dir.join("src/main/fortytwolexer.mbt").exists());
}
