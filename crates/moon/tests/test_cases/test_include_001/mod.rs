use super::*;

#[test]
fn test_include_001() {
    run_moon_cmdtest("test_include_001.in");
}

#[test]
fn test_deprecated_packaging_config_warns_during_check() {
    let dir = TestDir::new("test_include_001.in");
    let stderr = get_stderr(&dir, ["check"]);
    let warning = "`include` in `$ROOT/moon.mod.json` is deprecated";
    assert_eq!(
        stderr.matches(warning).count(),
        1,
        "expected exactly one deprecation warning, got:\n{stderr}"
    );
}

#[test]
fn test_deprecated_packaging_config_warns_during_fmt() {
    let dir = TestDir::new("test_include_001.in");
    let stderr = get_stderr(&dir, ["fmt", "--dry-run"]);
    let warning = "`include` in `$ROOT/moon.mod.json` is deprecated";
    assert_eq!(
        stderr.matches(warning).count(),
        1,
        "expected exactly one deprecation warning, got:\n{stderr}"
    );
}
