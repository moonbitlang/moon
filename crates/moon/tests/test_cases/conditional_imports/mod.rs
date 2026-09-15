use super::*;

#[test]
fn test_conditional_imports_cli_planning() {
    let dir = TestDir::new("conditional_imports/fixture");
    // Compiler and formatter support for #cfg in moon.pkg is delivered by the
    // toolchain. Exercise Moon's multi-backend pipeline without invoking them.
    for args in [
        vec!["check", "--target", "all", "--dry-run"],
        vec!["test", "app", "--target", "all", "--dry-run"],
        vec!["run", "main", "--target", "native", "--dry-run"],
        vec!["tree", "--package"],
    ] {
        moon_cmd(&dir).args(args).assert().success().stderr_eq("");
    }
}

#[test]
fn test_conditional_import_errors_only_fail_requested_backends() {
    let dir = TestDir::new("conditional_imports/fixture");
    std::fs::write(
        dir.join("app/moon.pkg"),
        r#"
supported_targets = "all"
#cfg(target = "native")
import { "example/conditional/missing" }
"#,
    )
    .unwrap();

    moon_cmd(&dir)
        .args(["check", "--target", "js", "--dry-run"])
        .assert()
        .success()
        .stderr_eq("");

    for args in [
        vec!["check", "--target", "native", "--dry-run"],
        vec!["check", "--target", "all", "--dry-run"],
        vec!["tree", "--package"],
    ] {
        moon_cmd(&dir)
            .args(args)
            .assert()
            .failure()
            .stderr_eq(snapbox::str![[r#"
...
[..]Cannot find import 'example/conditional/missing' in example/conditional/app[..]
"#]]);
    }
}
