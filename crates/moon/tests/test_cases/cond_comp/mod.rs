use std::path::PathBuf;

#[test]
fn test_cond_comp() {
    let xtask_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/debug/xtask");
    let test_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/test_cases/cond_comp.in/moon.test");

    let status = std::process::Command::new(xtask_path)
        .arg("cmdtest")
        .arg(test_path)
        .status()
        .unwrap();

    assert!(status.success());
}
