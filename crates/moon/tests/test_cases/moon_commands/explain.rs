use crate::util::check;
use crate::{TestDir, get_err_stderr, get_stdout};
use expect_test::expect;

#[test]
fn test_moon_explain_diagnostic_lists_compiler_warnings_and_integrated_docs() {
    let dir = TestDir::new_empty();
    let output = get_stdout(&dir, ["explain", "--diagnostic"]);
    assert!(output.starts_with("Available warnings: \n"));
    assert!(output.contains("partial_match              Partial pattern matching.                                       11 error"));
    assert!(output.contains("note: default alert exceptions: alert_unsafe=off"));
    assert!(output.contains("Available non-warning diagnostics:"));
    assert!(output.contains("E4056  diagnostic method_duplicate"));
}

#[test]
fn test_moon_explain_diagnostics_number_uses_integrated_docs() {
    let dir = TestDir::new_empty();
    let output = get_stdout(&dir, ["explain", "--diagnostic", "2"]);
    assert!(output.starts_with("# E0002\n"));
    assert!(output.contains("Warning name: `unused_value`"));
    assert!(output.contains("Unused variable."));
}

#[test]
fn test_moon_explain_diagnostics_alias_uses_integrated_docs() {
    let dir = TestDir::new_empty();
    let output = get_stdout(&dir, ["explain", "--diagnostics", "2"]);
    assert!(output.starts_with("# E0002\n"));
    assert!(output.contains("Warning name: `unused_value`"));
}

#[test]
fn test_moon_explain_diagnostics_mnemonic_uses_integrated_docs() {
    let dir = TestDir::new_empty();
    let output = get_stdout(&dir, ["explain", "--diagnostic", "unused_value"]);
    assert!(output.contains("# E0001"));
    assert!(output.contains("# E0002"));
}

#[test]
fn test_moon_explain_diagnostics_name_uses_integrated_docs() {
    let dir = TestDir::new_empty();
    let output = get_stdout(&dir, ["explain", "--diagnostic", "method_duplicate"]);
    assert!(output.starts_with("# E4056\n"));
    assert!(output.contains("Compiler diagnostic name: `method_duplicate`"));
}

#[test]
fn test_moon_explain_attribute_lists_integrated_docs() {
    let dir = TestDir::new_empty();
    let output = get_stdout(&dir, ["explain", "--attribute"]);
    assert!(output.starts_with("Available attributes:\n"));
    assert!(output.contains("#alert"));
    assert!(output.contains("#coverage.skip"));
    assert!(output.contains("#visibility"));
}

#[test]
fn test_moon_explain_attribute_name_uses_integrated_docs() {
    let dir = TestDir::new_empty();
    let output = get_stdout(&dir, ["explain", "--attribute", "alert"]);
    assert!(output.starts_with("# Alert Attribute\n"));
    assert!(output.contains("The `#alert` attribute attaches a category and message to an API."));
}

#[test]
fn test_moon_explain_without_flags_shows_guidance() {
    let dir = TestDir::new_empty();
    check(
        get_err_stderr(&dir, ["explain"]),
        expect![[r#"
            Explain compiler diagnostics and language topics

            Usage: moon explain [OPTIONS] <--diagnostic [<ID_OR_NAME>]|--attribute [<NAME>]>

            Options:
                  --diagnostic [<ID_OR_NAME>]  Explain diagnostics. Without a query, list diagnostic codes and names
                  --attribute [<NAME>]         Explain attributes. Without a query, list attribute names
              -h, --help                       Print help

            Common Options:
                  --target-dir <TARGET_DIR>  The target directory. Defaults to `<project-root>/_build`, or `<source-dir>/_build/<file-name>` for a standalone file
              -q, --quiet                    Suppress output
              -v, --verbose                  Increase verbosity
                  --trace                    Trace the execution of the program
                  --dry-run                  Do not actually run the command

            Resources:
                Docs: https://docs.moonbitlang.com
                Skills: https://github.com/moonbitlang/skills

                Use `moon explain --diagnostic` to list diagnostic codes and names.
                Use `moon explain --attribute` to list attributes.
        "#]],
    );
}
