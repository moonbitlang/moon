use super::*;
use expect_test::expect;

#[test]
fn test_diamond_pkg_001() {
    let dir = TestDir::new("diamond_pkg/001");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["build"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            A
            B
            A
            C
            main
        "#]],
    );
}

#[test]
fn test_diamond_pkg_002() {
    let dir = TestDir::new("diamond_pkg/002");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["build"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            A0
            A1
            A2
            A
            B0
            B1
            B2
            B
            A0
            A1
            A2
            A
            C0
            C1
            C2
            C
            main
        "#]],
    );
}

#[test]
fn test_diamond_pkg_003() {
    let dir = TestDir::new("diamond_pkg/003");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["build"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            A0
            A1
            A2
            A
            B0
            B1
            B2
            B
            A0
            A1
            A2
            A
            C0
            C1
            C2
            C
            main
        "#]],
    );
}
