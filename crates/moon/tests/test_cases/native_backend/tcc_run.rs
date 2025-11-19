use crate::TestDir;
use expect_test::expect_file;

use super::assert_native_backend_graph_no_env;

#[test]
#[cfg(unix)]
fn test_native_backend_tcc_run() {
    let dir = TestDir::new("native_backend/tcc_run");
    assert_native_backend_graph_no_env(
        &dir,
        "build_native_graph.jsonl",
        &["build", "--target", "native", "--dry-run", "--sort-input"],
        expect_file!["tcc_run/build_native_graph.jsonl.snap"],
    );

    assert_native_backend_graph_no_env(
        &dir,
        "test_native_linux_graph.jsonl",
        &["test", "--target", "native", "--dry-run", "--sort-input"],
        expect_file!["tcc_run/test_native_linux_graph.jsonl.snap"],
    );
}
