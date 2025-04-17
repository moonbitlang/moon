use super::*;
#[test]
fn test_inline_test_001() {
    let dir = TestDir::new("inline_test/001");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["test"])
        .assert()
        .success();
}

#[test]
fn test_inline_test_002() {
    let dir = TestDir::new("inline_test/002");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["test"])
        .assert()
        .success();
}

#[test]
fn test_inline_test_003() {
    let dir = TestDir::new("inline_test/003");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["test"])
        .assert()
        .failure();
}

#[test]
fn test_inline_test_004() {
    let dir = TestDir::new("inline_test/004");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["test"])
        .assert()
        .failure();
}

#[test]
fn test_inline_test_order() {
    let dir = TestDir::new("inline_test/order");
    check(
        get_stdout(&dir, ["test", "-v", "--sort-input", "--no-parallelize"]),
        expect![[r#"
            executing A
            executing A::hello.mbt::test_A
            test username/hello/A/hello.mbt::0 ok
            test username/hello/A/hello.mbt::1 ok
            A_test.mbt::init
            A_test.mbt::test_hello_A
            test username/hello/A/A_wbtest.mbt::0 ok
            test username/hello/A/A_wbtest.mbt::1 ok
            executing B
            executing B::hello.mbt::test_B
            test username/hello/B/hello.mbt::0 ok
            test username/hello/B/hello.mbt::1 ok
            B_test.mbt::init
            B_test.mbt::test_hello_B
            test username/hello/B/B_wbtest.mbt::0 ok
            test username/hello/B/B_wbtest.mbt::1 ok
            Total tests: 8, passed: 8, failed: 0.
        "#]],
    );

    check(
        get_stdout(&dir, ["run", "main", "--sort-input"]),
        expect![[r#"
            main.mbt::init
        "#]],
    );
}
