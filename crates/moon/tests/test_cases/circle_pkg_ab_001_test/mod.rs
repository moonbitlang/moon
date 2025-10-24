use crate::{TestDir, get_err_stderr};

#[test]
fn circle_pkg_test() {
    let dir = TestDir::new("circle_pkg_ab_001_test");
    let stderr = get_err_stderr(&dir, ["run", "main", "--nostd"]);
    assert!(stderr.contains("cyclic dependency"), "stderr: {stderr}");
}
