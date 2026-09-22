use super::*;

#[test]
fn test_conditional_imports_cli_planning() {
    let dir = TestDir::new("conditional_imports/fixture");
    // Compiler and formatter support for #cfg in moon.pkg is delivered by the
    // toolchain. Exercise Moon's multi-backend pipeline without invoking them.
    for args in [
        vec!["build", "--target", "all", "--dry-run"],
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
    for (broken, valid) in [("js", "native"), ("native", "js")] {
        std::fs::write(
            dir.join("app/moon.pkg"),
            format!(
                r#"supported_targets = "all"
#cfg(target = "{broken}")
import {{ "example/conditional/missing" }}
"#
            ),
        )
        .unwrap();

        moon_cmd(&dir)
            .args(["check", "--target", valid, "--dry-run"])
            .assert()
            .success()
            .stderr_eq("");

        for args in [
            vec!["check", "--target", broken, "--dry-run"],
            vec!["check", "--target", "js,native", "--dry-run"],
            vec!["check", "--target", "native,js", "--dry-run"],
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
}

#[test]
fn test_implicit_backends_follow_selected_workspace_packages() {
    let dir = TestDir::new("workspace_conflicting_preferred_targets.in");
    std::fs::write(
        dir.join("js_preferred/src/lib/moon.pkg"),
        r#"supported_targets = "all"
#cfg(target = "js")
import { "missing/conditional/package" }
"#,
    )
    .unwrap();

    std::fs::remove_file(dir.join("js_preferred/src/lib/moon.pkg.json")).unwrap();

    for command in ["build", "check", "test", "bench"] {
        moon_cmd(&dir)
            .args([command, "native_preferred/src/lib", "--dry-run"])
            .assert()
            .success()
            .stderr_eq(snapbox::str![[r#"
Warning: `moon.mod.json` at '[..]/js_preferred' is deprecated. Run `moon fmt` to migrate to `moon.mod`.
Warning: `moon.mod.json` at '[..]/native_preferred' is deprecated. Run `moon fmt` to migrate to `moon.mod`.

"#]]);
    }
    moon_cmd(&dir)
        .args(["check", "--dry-run"])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
...
[..]Cannot find import 'missing/conditional/package' in [..]
"#]]);
}

#[test]
fn test_conditional_import_support_error_describes_active_graph() {
    let dir = TestDir::new("conditional_imports/fixture");
    std::fs::write(
        dir.join("app/moon.pkg"),
        r#"
supported_targets = "all"
#cfg(target = "native")
import { "example/conditional/portable" }
#cfg(target = "js")
import { "example/conditional/native" }
"#,
    )
    .unwrap();

    moon_cmd(&dir)
        .args(["check", "main", "--target", "js", "--dry-run"])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: failed to run check for target Js

Caused by:
    0: Failed to calculate build plan
    1: Failed to build a build plan for the modules
    2: Selected backend 'js' is incompatible with the dependency graph. 'example/conditional/main' requires 'example/conditional/app' whose active dependency graph supports [native]. Dependency path: example/conditional/main -> example/conditional/app

"#]]);
}

#[test]
fn test_conditional_import_warnings_only_report_for_selected_backend() {
    let dir = TestDir::new("conditional_imports/fixture");
    std::fs::write(
        dir.join("app/moon.pkg"),
        r#"
supported_targets = "all"
#cfg(target = "native")
import { "example/conditional/native" }
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("native/moon.pkg"),
        "supported_targets = \"native\"\npkgtype(kind: \"executable\")\n",
    )
    .unwrap();

    moon_cmd(&dir)
        .args(["check", "app", "--target", "js", "--dry-run"])
        .assert()
        .success()
        .stderr_eq("");
    moon_cmd(&dir)
        .args(["check", "app", "--target", "native", "--dry-run"])
        .assert()
        .success()
        .stderr_eq(snapbox::str![[r#"
Warning: Package `example/conditional/app` depends on main package `example/conditional/native` via `import` in package directory "[..]/app". This dependency will become an error in a future release. Move reusable APIs into a non-main package and keep main packages as entrypoints.

"#]]);
}

#[test]
fn test_unconditional_import_warning_is_not_repeated_for_each_backend() {
    let dir = TestDir::new("conditional_imports/fixture");
    std::fs::write(
        dir.join("app/moon.pkg"),
        "supported_targets = \"all\"\nimport { \"example/conditional/main\" }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("main/moon.pkg"),
        "supported_targets = \"all\"\npkgtype(kind: \"executable\")\n",
    )
    .unwrap();

    moon_cmd(&dir)
        .args(["check", "app", "--target", "all", "--dry-run"])
        .assert()
        .success()
        .stderr_eq(snapbox::str![[r#"
Warning: Package `example/conditional/app` depends on main package `example/conditional/main` via `import` in package directory "[..]/app". This dependency will become an error in a future release. Move reusable APIs into a non-main package and keep main packages as entrypoints.

"#]]);
}
