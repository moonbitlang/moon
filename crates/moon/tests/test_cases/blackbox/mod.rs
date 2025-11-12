use expect_test::expect_file;

use crate::{TestDir, build_graph::compare_graphs, snap_dry_run_graph};

#[test]
fn test_blackbox_test_core_override() {
    let dir = TestDir::new("blackbox_test_core_override.in");

    let graph = dir.join("out.jsonl");
    let output = snap_dry_run_graph(
        &dir,
        ["test", "--enable-coverage", "--dry-run", "--sort-input"],
        &graph,
    );
    compare_graphs(
        &graph,
        expect_file!["test_blackbox_test_core_override.jsonl.snap"],
    );

    let mut found = false;
    for line in output.lines() {
        // For the command compiling builtin's blackbox tests,
        if line.contains("moonc build-package") && line.contains("builtin_blackbox_test") {
            found = true;
            // it should not have the -enable-coverage flag
            assert!(
                !line.contains("-enable-coverage"),
                "Black box tests themselves should not contain coverage, since all they contain are tests of various kinds. {line}"
            );
            // and should not contain -coverage-package-override to itself
            assert!(
                !line.contains("-coverage-package-override=@self"),
                "Unexpected -coverage-package-override=@self found in the command: {line}"
            );
        }
    }
    assert!(found, "builtin's blackbox tests not found in the output");
}
