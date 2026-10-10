use expect_test::{expect, expect_file};

use crate::{TestDir, build_graph, get_stderr, get_stdout, moon_cmd, util::check};

#[test]
fn test_third_party() {
    let dir = TestDir::new("third_party");
    get_stdout(&dir, ["update"]);
    get_stdout(&dir, ["build", "--target", "wasm-gc"]);
    get_stdout(&dir, ["clean"]);

    let actual = get_stderr(&dir, ["check", "--target", "wasm-gc"]);
    expect![[r#"
        Finished. moon: ran 5 tasks, now up to date
    "#]]
    .assert_eq(&actual);

    build_graph::assert(
        moon_cmd(&dir).args(["test", "--target", "wasm-gc", "--dry-run", "--sort-input"]),
        expect_file!["third_party_dry_run.jsonl"],
    );

    check(
        get_stdout(&dir, ["test", "--target", "wasm-gc", "--sort-input"]),
        expect![[r#"
            Hello, world!
            Hello, world!
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    let actual = get_stderr(&dir, ["build", "--target", "wasm-gc"]);
    expect![[r#"
        Finished. moon: ran 3 tasks, now up to date
    "#]]
    .assert_eq(&actual);

    let actual = get_stdout(&dir, ["run", "--target", "wasm-gc", "main"]);
    assert!(actual.contains("Hello, world!"));
}
