use super::*;

#[cfg(unix)]
#[test]
fn test_expect_test() -> anyhow::Result<()> {
    let tmp_dir_path = TestDir::new("test_expect_test/expect_test");

    let s = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&tmp_dir_path)
        .args(["test", "--update", "--no-parallelize"])
        .assert()
        .success()
        .get_output()
        .stdout
        .to_owned();
    let out = std::str::from_utf8(&s).unwrap().to_string();

    assert!(out.contains("Auto updating expect tests and retesting ..."));
    assert!(out.contains("Total tests: 30, passed: 30, failed: 0."));
    let updated =
        std::fs::read_to_string(tmp_dir_path.as_ref().join("lib").join("hello.mbt")).unwrap();
    assert!(updated.contains(r#"["a", "b", "c"]"#));

    let s = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&tmp_dir_path)
        .args(["test", "--update", "--no-parallelize"])
        .assert()
        .success()
        .get_output()
        .stdout
        .to_owned();

    let out = std::str::from_utf8(&s).unwrap().to_string();
    assert!(out.contains("Total tests: 30, passed: 30, failed: 0."));
    let out = std::fs::read_to_string(tmp_dir_path.as_ref().join("lib").join("hello_wbtest.mbt"))
        .unwrap();
    assert!(out.contains(r#"inspect(notbuf, content="haha")"#));
    Ok(())
}

#[test]
fn test_only_update_expect() {
    let tmp_dir_path = TestDir::new("test_expect_test/only_update_expect");

    let _ = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&tmp_dir_path)
        .args([
            "test",
            "-p",
            "username/hello/lib",
            "-f",
            "hello.mbt",
            "-i",
            "0",
            "--update",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .to_owned();
}
