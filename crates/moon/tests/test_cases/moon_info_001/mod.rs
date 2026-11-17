use super::*;

#[test]
fn test_moon_info_001() {
    run_moon_cmdtest("moon_info_001.in");
}

#[test]
fn test_moon_info_user_log_output() {
    let dir = TestDir::new("moon_info_001.in");
    moon_cmd(&dir)
        .arg("info")
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
Finished. moon: ran 4 tasks, now up to date

"#]]);

    let quiet_dir = TestDir::new("moon_info_001.in");
    moon_cmd(&quiet_dir)
        .args(["info", "--quiet", "--verbose"])
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq("");
}
