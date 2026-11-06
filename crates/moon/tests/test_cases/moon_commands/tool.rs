#[cfg(unix)]
use crate::moon_cmd;
use crate::util::check;
use crate::{TestDir, get_stdout};
use expect_test::expect;

#[test]
#[cfg(unix)]
fn test_tool_exec_shell_applies_cwd_env_and_execs() {
    let dir = TestDir::new_empty();
    let cwd = dir.join("exec-cwd");
    std::fs::create_dir_all(&cwd).expect("failed to create exec cwd");

    moon_cmd(&dir)
        .env("MOON_EXEC_REMOVE", "remove-me")
        .args(["tool", "exec", "--cwd"])
        .arg(&cwd)
        .args([
            "--env",
            "MOON_EXEC_TEST=expected",
            "--unset-env",
            "MOON_EXEC_REMOVE",
            "--shell",
            "pwd -P > cwd.txt; printf '%s' \"$MOON_EXEC_TEST\" > env.txt; printf '%s' \"${MOON_EXEC_REMOVE-unset}\" > unset-env.txt; printf '%s' \"$PPID\" > ppid.txt",
        ])
        .assert()
        .success();

    let actual_cwd = std::fs::read_to_string(cwd.join("cwd.txt"))
        .expect("cwd marker should be written")
        .trim()
        .to_string();
    let expected_cwd = std::fs::canonicalize(&cwd)
        .expect("cwd should canonicalize")
        .display()
        .to_string();
    assert_eq!(actual_cwd, expected_cwd);
    assert_eq!(
        std::fs::read_to_string(cwd.join("env.txt")).expect("env marker should be written"),
        "expected"
    );
    assert_eq!(
        std::fs::read_to_string(cwd.join("unset-env.txt"))
            .expect("unset env marker should be written"),
        "unset"
    );

    let ppid = std::fs::read_to_string(cwd.join("ppid.txt"))
        .expect("ppid marker should be written")
        .parse::<u32>()
        .expect("ppid marker should be a process id");
    assert_eq!(ppid, std::process::id());
}

#[test]
fn test_moon_tool_demangle() {
    let dir = TestDir::new_empty();
    check(
        get_stdout(
            &dir,
            [
                "tool",
                "demangle",
                "_M0FP38username5hello4lib05hello",
                "_M0Lm7$foo.fnS12",
                "plain",
            ],
        ),
        expect![[r#"
            @username/hello/lib0.hello
            foo/12
            plain
        "#]],
    );
}
