use crate::util::{moon_bin, moonrun_bin};
use crate::{TestDir, get_err_stderr_with_envs, get_stdout, moon_cmd};

#[test]
fn test_run_direct_wasm_forwards_policy() {
    let dir = TestDir::new("moon_run_with_cli_args.in");
    moon_cmd(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let stderr = get_err_stderr_with_envs(
        &dir,
        [
            "run",
            "_build/wasm/debug/build/main/main.wasm",
            "--wasm-policy",
            "missing-policy.json",
        ],
        [("MOONRUN_OVERRIDE", moonrun_bin())],
    );
    assert!(
        stderr.contains("failed to load sandbox policy"),
        "expected direct Wasm execution to forward the policy:\n{stderr}"
    );
}

#[test]
fn test_run_with_explicit_target_exits_with_guest_exit_code() {
    let dir = TestDir::new("moon_run_with_cli_args.in");
    moon_cmd(&dir)
        .env("MOONRUN_OVERRIDE", moonrun_bin())
        .args(["run", "main", "--target", "wasm", "exit-7"])
        .assert()
        .code(7)
        .stdout_eq("");
}

#[test]
fn test_moon_stdin_without_args_fails() {
    let dir = TestDir::new_empty();
    let out = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .stdin(
            r#"fn main {
  println("hello from piped stdin")
}
"#,
        )
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();
    let stderr = String::from_utf8_lossy(&out).replace("moon.exe", "moon");
    assert!(stderr.contains("Usage: moon [OPTIONS] <COMMAND>"));
}

#[test]
fn test_moon_dash_shows_run_stdin_guidance() {
    let dir = TestDir::new_empty();
    let out = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("-")
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();
    let stderr = String::from_utf8_lossy(&out);
    assert!(stderr.contains("moon run -"));
    assert!(stderr.contains(".mbtx"));
}

#[test]
fn test_moon_run_dash_reads_stdin() {
    let dir = TestDir::new_empty();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("run")
        .arg("-")
        .stdin(
            r#"fn main {
  println("hello from run dash stdin")
}
"#,
        )
        .assert()
        .success()
        .stdout_eq("hello from run dash stdin\n");
}

#[test]
fn test_moon_run_help_displays_inline_script_as_e() {
    let dir = TestDir::new_empty();
    let stdout = get_stdout(&dir, ["help", "run"]).replace("moon.exe", "moon");
    assert!(stdout.contains("-e <SCRIPT>"), "stdout: {stdout}");
    assert!(!stdout.contains("-c <SCRIPT>"), "stdout: {stdout}");
}

#[test]
fn test_moon_run_command_string_reads_inline_script() {
    let dir = TestDir::new_empty();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("run")
        .arg("-e")
        .arg(
            r#"fn main {
  println("hello from run -e")
}
"#,
        )
        .assert()
        .success()
        .stdout_eq("hello from run -e\n")
        .stderr_eq("");
}

#[cfg(unix)]
#[test]
fn test_moon_run_command_string_removes_temporary_project() {
    let dir = TestDir::new_empty();
    let temp_root = dir.join("tmp");
    std::fs::create_dir(&temp_root).unwrap();

    moon_cmd(&dir)
        .env("TMPDIR", &temp_root)
        .args([
            "run",
            "-e",
            r#"fn main {
  println("hello")
}
"#,
        ])
        .assert()
        .success()
        .stdout_eq("hello\n");

    assert_eq!(std::fs::read_dir(temp_root).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn test_moon_run_flushes_trace() {
    let dir = TestDir::new("moon_commands");

    moon_cmd(&dir)
        .args(["run", "--trace", "main1"])
        .assert()
        .success();

    let trace = std::fs::read_to_string(dir.join("trace.json")).unwrap();
    let events: serde_json::Value = serde_json::from_str(&trace).unwrap();
    assert!(events.as_array().is_some_and(|events| !events.is_empty()));
}

#[test]
fn test_moon_run_command_string_short_alias_c_reads_inline_script() {
    let dir = TestDir::new_empty();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("run")
        .arg("-c")
        .arg(
            r#"fn main {
  println("hello from run -c")
}
"#,
        )
        .assert()
        .success()
        .stdout_eq("hello from run -c\n")
        .stderr_eq("");
}

#[test]
fn test_moon_run_command_string_invalid_source_keeps_diagnostics_on_stderr() {
    let dir = TestDir::new_empty();
    let assert = moon_cmd(&dir)
        .args(["run", "-e", r#"println("hello")"#])
        .assert()
        .failure()
        .stdout_eq("");
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("Missing main function in the main package"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("Parse error, unexpected token"),
        "stderr: {stderr}"
    );
    assert!(!stderr.contains("failed:"), "stderr: {stderr}");
}

#[test]
fn test_moon_run_command_string_invalid_source_shows_failed_command_on_stderr_in_verbose_mode() {
    let dir = TestDir::new_empty();
    moon_cmd(&dir)
        .args(["run", "--verbose", "-e", r#"println("hello")"#])
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
Failed command: [..]moonc[..] build-package [..]
Error: [4067] Missing main function in the main package.
Error: [3002]
...

"#]]);
}

#[test]
fn test_moon_run_command_string_defaults_to_wasm() {
    let dir = TestDir::new_empty();
    let stdout = get_stdout(
        &dir,
        [
            "run",
            "-e",
            r#"fn main {
  println("hello from run -e")
}
"#,
            "--dry-run",
        ],
    );

    assert!(stdout.contains("-target wasm"), "stdout: {stdout}");
    assert!(
        stdout.contains("./_build/command.mbtx/wasm/debug/build/single/single.core"),
        "stdout: {stdout}"
    );
    assert!(!stdout.contains("-target wasm-gc"), "stdout: {stdout}");
    assert!(!stdout.contains("-target native"), "stdout: {stdout}");
}

#[test]
fn test_moon_run_command_string_explicit_target_overrides_wasm_default() {
    let dir = TestDir::new_empty();
    let stdout = get_stdout(
        &dir,
        [
            "run",
            "-e",
            r#"fn main {
  println("hello from run -e")
}
"#,
            "--target",
            "wasm-gc",
            "--dry-run",
        ],
    );

    assert!(stdout.contains("-target wasm-gc"), "stdout: {stdout}");
    assert!(
        stdout.contains(
            "'$MOONRUN_OVERRIDE' ./_build/command.mbtx/wasm-gc/debug/build/single/single.wasm --"
        ),
        "stdout: {stdout}"
    );
    assert!(
        !stdout.contains("./_build/command.mbtx/wasm/debug/"),
        "stdout: {stdout}"
    );
    assert!(!stdout.contains("-target native"), "stdout: {stdout}");
}

#[test]
fn test_moon_run_command_string_short_alias_c_defaults_to_wasm() {
    let dir = TestDir::new_empty();
    let stdout = get_stdout(
        &dir,
        [
            "run",
            "-c",
            r#"fn main {
  println("hello from run -c")
}
"#,
            "--dry-run",
        ],
    );

    assert!(stdout.contains("-target wasm"), "stdout: {stdout}");
    assert!(
        stdout.contains("./_build/command.mbtx/wasm/debug/build/single/single.core"),
        "stdout: {stdout}"
    );
    assert!(!stdout.contains("-target wasm-gc"), "stdout: {stdout}");
    assert!(!stdout.contains("-target native"), "stdout: {stdout}");
}

#[test]
fn test_moon_run_command_string_conflicts_with_other_inputs() {
    let dir = TestDir::new_empty();

    let dash_stderr = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("run")
        .arg("-e")
        .arg(r#"fn main { println("hello") }"#)
        .arg("-")
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();
    let dash_stderr = String::from_utf8_lossy(&dash_stderr);
    assert!(dash_stderr.contains("cannot be used with"));
    assert!(dash_stderr.contains("-e"));

    let path_stderr = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("run")
        .arg("-e")
        .arg(r#"fn main { println("hello") }"#)
        .arg("main")
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();
    let path_stderr = String::from_utf8_lossy(&path_stderr);
    assert!(path_stderr.contains("cannot be used with"));
    assert!(path_stderr.contains("-e"));
}

#[test]
fn test_moon_run_dash_reads_stdin_with_common_flags() {
    let dir = TestDir::new_empty();
    let subdir = dir.join("subdir");
    std::fs::create_dir_all(&subdir).unwrap();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("-C")
        .arg(&subdir)
        .arg("run")
        .arg("-")
        .stdin(
            r#"fn main {
  println("hello from run dash with -C")
}
"#,
        )
        .assert()
        .success()
        .stdout_eq("hello from run dash with -C\n");
}

#[test]
fn test_moon_run_dash_reads_stdin_with_build_flags() {
    let dir = TestDir::new_empty();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("run")
        .arg("--release")
        .arg("-")
        .stdin(
            r#"fn main {
  println("hello from run dash with --release")
}
"#,
        )
        .assert()
        .success()
        .stdout_eq("hello from run dash with --release\n");
}

#[cfg(unix)]
#[test]
fn test_moon_run_dash_with_heredoc() {
    let dir = TestDir::new_empty();
    let moon = moon_bin().to_string_lossy().replace('\'', "'\\''");
    let script = format!(
        r#"MOON_BIN='{moon}'
"$MOON_BIN" run - <<'EOF'
fn main {{
  println("hello from run heredoc")
}}
EOF
"#
    );
    let output = std::process::Command::new("sh")
        .current_dir(&dir)
        .arg("-c")
        .arg(script)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "status: {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hello from run heredoc"));
}

#[cfg(unix)]
#[test]
fn test_moon_run_pipe_input() {
    let dir = TestDir::new_empty();
    let moon = moon_bin().to_string_lossy().replace('\'', "'\\''");
    let script = format!(
        r#"MOON_BIN='{moon}'
echo '----
import {{
  "moonbitlang/core/list",
}}
----
fn main {{
  let xs : @list.List[Int] = @list.of([1, 2, 3])
  println("hello from run pipe \{{xs}}")
}}' | "$MOON_BIN" run -
"#
    );
    let output = std::process::Command::new("sh")
        .current_dir(&dir)
        .arg("-c")
        .arg(script)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "status: {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hello from run pipe"));
}
