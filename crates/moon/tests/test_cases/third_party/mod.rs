use expect_test::{expect, expect_file};

use crate::{
    TestDir, build_graph::compare_graphs, get_stderr, get_stdout, snap_dry_run_graph, util::check,
};

#[test]
fn test_third_party() {
    let dir = TestDir::new("third_party");
    get_stdout(&dir, ["update"]);
    get_stdout(&dir, ["install"]);
    get_stdout(&dir, ["build"]);
    get_stdout(&dir, ["clean"]);

    let actual = get_stderr(&dir, ["check"]);
    expect![[r#"
        Finished. moon: ran 6 tasks, now up to date
    "#]]
    .assert_eq(&actual);

    let file = dir.join("test_dry_run.jsonl");
    snap_dry_run_graph(&dir, ["test", "--dry-run", "--sort-input"], &file);
    compare_graphs(&file, expect_file!["third_party_dry_run.jsonl"]);

    check(
        get_stdout(&dir, ["test", "--sort-input"]),
        expect![[r#"
            Hello, world!
            Hello, world!
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    let actual = get_stderr(&dir, ["build"]);
    expect![[r#"
        Finished. moon: ran 3 tasks, now up to date
    "#]]
    .assert_eq(&actual);

    let actual = get_stdout(&dir, ["run", "main"]);
    assert!(actual.contains("Hello, world!"));
}
