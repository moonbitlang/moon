use expect_test::expect;

use crate::{TestDir, get_stderr, get_stdout, util::check};

#[test]
fn test_moon_test_json_output() {
    let dir = TestDir::new("warns/deny_warn");

    check(
        get_stdout(&dir, ["test", "--output-json", "--sort-input", "-j1", "-q"]),
        expect![""],
    );
}

#[test]
fn test_moon_test_rendered_output() {
    let dir = TestDir::new("warns/deny_warn");

    check(
        get_stderr(&dir, ["test", "--sort-input", "-j1", "-q"]),
        expect![[r#"
            Warning: no test entry found.
        "#]],
    );
}

#[test]
fn test_moon_test_raw_output() {
    let dir = TestDir::new("warns/deny_warn");

    check(
        get_stdout(&dir, ["test", "--no-render", "--sort-input", "-j1", "-q"]),
        expect![""],
    );
}
