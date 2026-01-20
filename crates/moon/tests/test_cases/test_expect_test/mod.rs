mod diff_format;

use super::*;

#[cfg(unix)]
#[test]
fn test_expect_test() -> anyhow::Result<()> {
    let tmp_dir_path = TestDir::new("test_expect_test/expect_test");

    let original =
        std::fs::read_to_string(tmp_dir_path.as_ref().join("lib").join("hello.mbt")).unwrap();
    println!("Original content:\n{}", original);
    assert!(!original.contains(r#"content=["a", "b", "c"]"#));

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
    let updated =
        std::fs::read_to_string(tmp_dir_path.as_ref().join("lib").join("hello.mbt")).unwrap();
    println!("Updated content:\n{}", updated);
    assert!(updated.contains(r#"#|["a", "b", "c"]"#));

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
    expect![[r#"
        test "hello" {
          let buf = @buffer.new()
          buf.write_string("just\ntest")
          inspect(buf, content=(
            
            #|just
            #|test
          ))
        }

        test "not-buf" {
          let notbuf = @buffer.new()
          notbuf.write_string("haha")
          inspect(notbuf, content=(
            #|haha
          ))
        }
    "#]]
    .assert_eq(&out);
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
            "--file",
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
