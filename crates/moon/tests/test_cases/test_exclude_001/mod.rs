use super::*;

#[test]
fn test_exclude_001() {
    run_moon_cmdtest("test_exclude_001.in");
}

#[test]
fn test_moonignore_overrides_gitignore() {
    let dir = TestDir::new("test_exclude_001.in");
    std::fs::write(dir.join("gitignored.txt"), "included by .moonignore\n").unwrap();
    std::fs::write(dir.join("moonignored.txt"), "excluded by .moonignore\n").unwrap();

    let stderr = get_stderr(&dir, ["package", "--list"]);
    assert!(
        stderr.lines().any(|line| line == "gitignored.txt"),
        ".moonignore should override the sibling .gitignore, got:\n{stderr}"
    );
    assert!(
        stderr.lines().all(|line| line != "moonignored.txt"),
        ".moonignore should still exclude its own matches, got:\n{stderr}"
    );
}
