use expect_test::expect_file;

use crate::{TestDir, get_err_stdout};

#[test]
fn test_expect_test_diff_format() {
    let test_dir = TestDir::new("test_expect_test/expect_test_diff_format");
    let stdout = get_err_stdout(&test_dir.path, ["test"]);
    expect_file!["./diff_format.snap"].assert_eq(&stdout);
}
