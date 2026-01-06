use super::*;

#[test]
fn test_clean() {
    let dir = TestDir::new("clean/clean.in");
    let _ = get_stdout(&dir, ["build"]);

    assert!(dir.join("target").exists());
    assert!(dir.join("_build").exists());

    let _ = get_stdout(&dir, ["clean"]);

    assert!(!(dir.join("target").exists()));
    assert!(!(dir.join("_build").exists()));
}
