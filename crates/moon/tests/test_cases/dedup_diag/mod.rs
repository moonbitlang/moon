use super::*;

fn trim_trailing_spaces(s: String) -> String {
    let mut trimmed = s.lines().map(str::trim_end).collect::<Vec<_>>().join("\n");
    if s.ends_with('\n') {
        trimmed.push('\n');
    }
    trimmed
}

#[test]
fn test_dedup_diag() {
    let dir = TestDir::new("dedup_diag.in");
    let out = get_stdout(&dir, ["test", "--output-json"]);

    check(
        out,
        expect![[r#"
            {"$message_type":"diagnostic","level":"warning","error_code":2,"path":"$ROOT/test.mbt","loc":"3:7-3:8","message":"Warning (unused_value): Unused variable 'a'","context":"2 |fn f() -> Unit {/n3 |  let a = 1/n4 |/n"}
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    )
}

#[test]
fn test_diagnostic_limit_output_json() {
    let dir = TestDir::new("dedup_diag_limit.in");
    let out = get_stdout(&dir, ["test", "--output-json", "--diagnostic-limit", "1"]);

    check(
        out,
        expect![[r#"
            {"$message_type":"diagnostic","level":"warning","error_code":2,"path":"$ROOT/test.mbt","loc":"3:7-3:8","message":"Warning (unused_value): Unused variable 'a'","context":"2 |fn f() -> Unit {/n3 |  let a = 1/n4 |  let b = 2/n"}
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    let dir = TestDir::new("dedup_diag_limit.in");
    let err = get_stderr(&dir, ["test", "--output-json", "--diagnostic-limit", "1"]);

    check(
        err,
        expect![[r#"
            Warning: diagnostic output limited by --diagnostic-limit: 0 errors and 1 warnings were not displayed.
        "#]],
    )
}

#[test]
fn test_diagnostic_limit_rendered_output() {
    let dir = TestDir::new("dedup_diag_limit.in");
    let err = trim_trailing_spaces(get_stderr(&dir, ["test", "--diagnostic-limit", "1"]));

    check(
        err,
        expect![[r#"
            Warning: [0002]
               ╭─[ $ROOT/test.mbt:3:7 ]
               │
             3 │   let a = 1
               │       ┬
               │       ╰── Warning (unused_value): Unused variable 'a'
            ───╯
            Warning: diagnostic output limited by --diagnostic-limit: 0 errors and 1 warnings were not displayed.
        "#]],
    )
}

#[test]
fn test_diagnostic_limit_prioritizes_errors() {
    let dir = TestDir::new("dedup_diag_error_limit.in");
    check(
        get_err_stdout(&dir, ["check", "--diagnostic-limit", "1"]),
        expect![[r#"
            Failed with 3 warnings, 1 errors.
        "#]],
    );

    let err = trim_trailing_spaces(get_err_stderr(&dir, ["check", "--diagnostic-limit", "1"]));

    check(
        err,
        expect![[r#"
            Error: [4021]
               ╭─[ $ROOT/z_error.mbt:3:3 ]
               │
             3 │   missing_identifier
               │   ─────────┬────────
               │            ╰────────── The value identifier missing_identifier is unbound.
            ───╯
            Warning: diagnostic output limited by --diagnostic-limit: 0 errors and 3 warnings were not displayed.
            Error: failed when checking project
        "#]],
    )
}

#[test]
fn test_json_diagnostics_do_not_include_human_failure_summary() {
    let dir = TestDir::new("dedup_diag_error_limit.in");
    let stdout = get_err_stdout(
        &dir,
        [
            "check",
            "--output-json",
            "--diagnostic-limit",
            "1",
            "--sort-input",
        ],
    );

    assert!(!stdout.contains("Failed with"), "stdout: {stdout}");
    assert!(
        stdout
            .lines()
            .all(|line| serde_json::from_str::<serde_json::Value>(line).is_ok()),
        "stdout must remain JSONL diagnostics: {stdout}"
    );
}
