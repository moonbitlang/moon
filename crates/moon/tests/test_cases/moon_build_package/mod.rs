use std::path::PathBuf;
use crate::util::xtask_bin;

#[test]
fn test_moon_build_package() {
    let xtask_path = xtask_bin();
    let test_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/test_cases/moon_build_package.in/moon.test");

    let status = std::process::Command::new(xtask_path)
        .arg("cmdtest")
        .arg(test_path)
        .status()
        .unwrap();

    assert!(status.success());
}
