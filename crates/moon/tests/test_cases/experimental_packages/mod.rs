use super::*;

#[test]
fn experimental_package_warnings_reach_build_commands() {
    let dir = TestDir::new("experimental_packages/fixture");
    // MoonBuild owns this warning. Dry runs exercise its command wiring without
    // requiring the compiler's manifest parser to recognize the new declaration.
    for args in [
        vec!["check", "--target", "all", "--dry-run"],
        vec!["build", "--dry-run"],
        vec!["test", "--dry-run"],
        vec!["run", "app", "--dry-run"],
    ] {
        moon_cmd(&dir)
            .args(args)
            .assert()
            .success()
            .stderr_eq(snapbox::str![[r#"
Warning: Package `example/experimental/app` depends on experimental package `example/experimental/unstable`. Set `options("allow-experimental-packages": ["example/experimental/unstable"])` in the importing package's moon.pkg to acknowledge this dependency.

"#]]);
    }
    moon_cmd(&dir)
        .args(["check", "--quiet", "--dry-run"])
        .assert()
        .success()
        .stderr_eq("");
}

#[test]
fn experimental_package_allowance_suppresses_cli_warning() {
    let dir = TestDir::new("experimental_packages/fixture");
    std::fs::write(
        dir.join("app/moon.pkg"),
        r#"pkgtype(kind: "executable")
import { "example/experimental/unstable" }
options("allow-experimental-packages": ["example/experimental/unstable"])
"#,
    )
    .unwrap();
    moon_cmd(&dir)
        .args(["check", "--target", "all", "--dry-run"])
        .assert()
        .success()
        .stderr_eq("");
}
