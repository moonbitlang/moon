use crate::util::check;
use crate::{TestDir, get_stdout_with_envs};
use expect_test::expect;

#[test]
fn test_moon_whoami_not_logged_in() {
    let dir = TestDir::new_empty();
    let moon_home = dir.join("moon_home");
    std::fs::create_dir_all(&moon_home).unwrap();
    check(
        get_stdout_with_envs(
            &dir,
            ["whoami"],
            [("MOON_HOME", moon_home.to_string_lossy().into_owned())],
        ),
        expect![[r#"
            Not logged in
        "#]],
    );
}

#[test]
fn test_moon_whoami_logged_in() {
    let dir = TestDir::new_empty();
    let moon_home = dir.join("moon_home");
    std::fs::create_dir_all(&moon_home).unwrap();
    std::fs::write(
        moon_home.join("credentials.json"),
        r#"{
  "token": "test-token",
  "username": "moonbit-user"
}
"#,
    )
    .unwrap();
    check(
        get_stdout_with_envs(
            &dir,
            ["whoami"],
            [("MOON_HOME", moon_home.to_string_lossy().into_owned())],
        ),
        expect![[r#"
            Logged in as moonbit-user
        "#]],
    );
}

#[test]
fn test_moon_whoami_without_username_suggests_relogin() {
    let dir = TestDir::new_empty();
    let moon_home = dir.join("moon_home");
    std::fs::create_dir_all(&moon_home).unwrap();
    std::fs::write(
        moon_home.join("credentials.json"),
        r#"{
  "token": "test-token"
}
"#,
    )
    .unwrap();
    check(
        get_stdout_with_envs(
            &dir,
            ["whoami"],
            [("MOON_HOME", moon_home.to_string_lossy().into_owned())],
        ),
        expect![[r#"
            Logged in, but username is unavailable. Please run `moon login` again.
        "#]],
    );
}
