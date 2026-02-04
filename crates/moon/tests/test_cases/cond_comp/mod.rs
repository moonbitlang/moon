use std::path::PathBuf;
use crate::util::xtask_bin;

#[test]
fn test_cond_comp() {
    let xtask_path = xtask_bin();
    let test_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases/cond_comp.in/moon.test");

    snapbox::cmd::Command::new(xtask_path)
        .arg("cmdtest")
        .arg(test_path)
        .assert()
        .success();
}
