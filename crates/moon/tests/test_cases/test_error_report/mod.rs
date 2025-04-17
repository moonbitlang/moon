use super::*;

#[test]
fn test_test_error_report() {
    let dir = TestDir::new("test_error_report.in");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["test"])
        .assert()
        .failure();
}
