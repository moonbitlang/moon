use super::*;

#[cfg(unix)]
#[test]
fn test_native_abort_trace() {
    let dir = TestDir::new("native_abort_trace/native_abort_trace.in");
    let redactions = moon_test_util::stack_trace::stack_trace_redactions(dir.as_ref());
    snapbox::cmd::Command::new(moon_bin())
        .with_assert(snapbox::Assert::new().redact_with(redactions))
        .current_dir(&dir)
        .args(["run", "--target", "native", "cmd/main"])
        .assert()
        .success()
        .stdout_eq("Hello\n")
        .stderr_eq(snapbox::str![[r#"
RUNTIME ERROR: abort() called
[CORE_PATH]/builtin/option.mbt[LINE_NUMBER] at @moonbitlang/core/option.Option::unwrap[Int]
[..]/cmd/main/main.mbt[LINE_NUMBER] by @username/scratch/cmd/main.g
[..]/cmd/main/main.mbt[LINE_NUMBER] by @username/scratch/cmd/main.f
[..]/cmd/main/main.mbt[LINE_NUMBER] by main

"#]]);
}
