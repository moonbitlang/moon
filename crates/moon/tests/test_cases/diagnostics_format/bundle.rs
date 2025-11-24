use expect_test::expect;

use crate::{TestDir, get_stderr, get_stdout, util::check};

#[test]
fn test_moon_bundle_json_output() {
    let dir = TestDir::new("warns/deny_warn");

    check(
        get_stdout(
            &dir,
            ["bundle", "--output-json", "--sort-input", "-j1", "-q"],
        ),
        expect![""],
    );
}

#[test]
fn test_moon_bundle_rendered_output() {
    let dir = TestDir::new("warns/deny_warn");

    check(
        get_stderr(&dir, ["bundle", "--sort-input", "-j1", "-q"]),
        expect![""],
    );
}

#[test]
fn test_moon_bundle_raw_output() {
    let dir = TestDir::new("warns/deny_warn");

    check(
        get_stdout(&dir, ["bundle", "--no-render", "--sort-input", "-j1", "-q"]),
        expect![""],
    );
}
