use crate::{TestDir, assert_dry_run_graph, get_stdout, util::check};
use expect_test::{expect, expect_file};

#[test]
fn test_moonbitlang_x() {
    let dir = TestDir::new("test_moonbitlang_x");
    get_stdout(&dir, ["update"]);
    get_stdout(&dir, ["install"]);

    assert_dry_run_graph(
        &dir,
        ["build", "--dry-run", "--sort-input"],
        expect_file!["moonbitlang_x_build_dry_run.jsonl.snap"],
    );

    assert_dry_run_graph(
        &dir,
        ["test", "--dry-run", "--sort-input"],
        expect_file!["moonbitlang_x_test_dry_run.jsonl.snap"],
    );

    check(
        get_stdout(&dir, ["run", "src/main"]),
        expect![[r#"
            Some(123)
        "#]],
    );
}
