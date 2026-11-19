use super::*;

#[test]
fn test_include_001() {
    run_moon_cmdtest("test_include_001.in");
}

#[test]
fn test_removed_packaging_config_errors_even_when_quiet() {
    let dir = TestDir::new("test_include_001.in");
    for field in ["include", "exclude"] {
        std::fs::write(
            dir.join("moon.mod"),
            format!("name = \"username/hello\"\nsource = \"src\"\noptions({field}: [])"),
        )
        .unwrap();
        for args in [
            ["check", "--quiet"].as_slice(),
            ["fmt", "--dry-run", "--quiet"].as_slice(),
        ] {
            moon_cmd(&dir)
                .args(args)
                .assert()
                .failure()
                .stderr_eq(snapbox::str![[r#"
...
[..]`[..]` is no longer supported in module manifests; use `.gitignore` or `.moonignore` to control packaged files. `.moonignore` overrides `.gitignore` in the same directory.
...
"#]]);
        }
    }
}
