use super::*;

#[test]
fn test_native_abort_trace() {
    let dir = TestDir::new("native_abort_trace/native_abort_trace.in");
    let redactions = moon_test_util::stack_trace::stack_trace_redactions(dir.as_ref());
    let expected_stderr = if cfg!(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )) {
        snapbox::str![[r#"
PanicError
    at @moonbitlang/core/option.Option::unwrap[Int] (/option.mbt:[..])
    at @username/scratch/cmd/main.g (/main.mbt:[..])
    at @username/scratch/cmd/main.f (/main.mbt:[..])
    at moonbit_main (/main.mbt:[..])
    at main (/main.mbt:[..])
...
Error: Command exited without a return code

"#]]
    } else {
        snapbox::str![[r#"
RUNTIME ERROR: abort() called
[CORE_PATH]/builtin/option.mbt[LINE_NUMBER] at @moonbitlang/core/option.Option::unwrap[Int]
[..]/cmd/main/main.mbt[LINE_NUMBER] by @username/scratch/cmd/main.g
[..]/cmd/main/main.mbt[LINE_NUMBER] by @username/scratch/cmd/main.f
[..]/cmd/main/main.mbt[LINE_NUMBER] by main

"#]]
    };
    snapbox::cmd::Command::new(moon_bin())
        .with_assert(snapbox::Assert::new().redact_with(redactions))
        .current_dir(&dir)
        .env_remove("MOONBIT_NEW_NATIVE")
        .args(["run", "--target", "native", "cmd/main"])
        .assert()
        .code(255)
        .stdout_eq("Hello\n")
        .stderr_eq(expected_stderr);
}
