use super::*;

#[test]
fn test_moon_info_001() {
    run_moon_cmdtest("moon_info_001.in");
}

#[test]
fn test_moon_info_user_log_output() {
    let dir = TestDir::new("moon_info_001.in");
    moon_cmd(&dir)
        .args(["info", "--no-alias"])
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq(
            "Warning: `--no-alias` will be removed soon. See: https://github.com/moonbitlang/moon/issues/1092\n",
        );

    let verbose_dir = TestDir::new("moon_info_001.in");
    moon_cmd(&verbose_dir)
        .args(["info", "--no-alias", "--verbose"])
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
Warning: `--no-alias` will be removed soon. See: https://github.com/moonbitlang/moon/issues/1092
...
Finished. moon: ran 4 tasks, now up to date

"#]]);

    let quiet_dir = TestDir::new("moon_info_001.in");
    moon_cmd(&quiet_dir)
        .args(["info", "--no-alias", "--quiet", "--verbose"])
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq("");
}
