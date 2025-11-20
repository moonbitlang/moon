use std::path::PathBuf;

#[test]
fn test_moon_info_002() {
    let xtask_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/debug/xtask");
    let test_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/test_cases/moon_info_002.in/moon.test");

    let status = std::process::Command::new(xtask_path)
        .arg("cmdtest")
        .arg(test_path)
        .status()
        .unwrap();

    assert!(status.success());
}
