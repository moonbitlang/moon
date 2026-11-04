use super::*;

#[test]
fn test_test_error_report() {
    let dir = TestDir::new("test_error_report");
    moon_cmd(&dir).args(["test"]).assert().failure();
}
