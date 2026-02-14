use super::*;

#[cfg(unix)]
#[test]
fn test_native_abort_trace() {
    const ANSI_LINE_NUMBER_REGEX: &str = r"(?<redacted>:[0-9]+)(?:(?:\x1b\[[0-9;]*m)|(?:\[ANSI_[A-Z]+\]))*(?:[ \t]+(?:(?:\x1b\[[0-9;]*m)|(?:\[ANSI_[A-Z]+\]))*(?:at|by)|\n|$)";
    let dir = TestDir::new("native_abort_trace/native_abort_trace.in");
    let mut redactions = moon_test_util::stack_trace::stack_trace_redactions(dir.as_ref());
    redactions
        .insert("[ANSI_RED]", "\u{1b}[31m")
        .expect("valid ANSI red redaction");
    redactions
        .insert("[ANSI_GRAY]", "\u{1b}[90m")
        .expect("valid ANSI gray redaction");
    redactions
        .insert("[ANSI_CYAN]", "\u{1b}[36m")
        .expect("valid ANSI cyan redaction");
    redactions
        .insert("[ANSI_BOLD]", "\u{1b}[1m")
        .expect("valid ANSI bold redaction");
    redactions
        .insert("[ANSI_RESET]", "\u{1b}[0m")
        .expect("valid ANSI reset redaction");
    redactions
        .insert(
            "[LINE_NUMBER]",
            moon_test_util::stack_trace::redaction_regex(ANSI_LINE_NUMBER_REGEX),
        )
        .expect("valid ANSI stack trace line number redaction");
    snapbox::cmd::Command::new(moon_bin())
        .with_assert(snapbox::Assert::new().redact_with(redactions))
        .current_dir(&dir)
        .env("NO_COLOR", "1")
        .env("CLICOLOR", "0")
        .args(["run", "--target", "native", "cmd/main"])
        .assert()
        .success()
        .stdout_eq("Hello\n")
        .stderr_eq(snapbox::str![[r#"
[ANSI_RED]RUNTIME ERROR: abort() called[ANSI_RESET]
[ANSI_GRAY][CORE_PATH]/builtin/option.mbt[LINE_NUMBER][ANSI_RESET] [ANSI_CYAN]at[ANSI_RESET] [ANSI_BOLD]@moonbitlang/core/option.Option::unwrap[Int][ANSI_RESET]
[ANSI_GRAY][..]/cmd/main/main.mbt[LINE_NUMBER][ANSI_RESET] [ANSI_CYAN]by[ANSI_RESET] [ANSI_BOLD]@username/scratch/cmd/main.g[ANSI_RESET]
[ANSI_GRAY][..]/cmd/main/main.mbt[LINE_NUMBER][ANSI_RESET] [ANSI_CYAN]by[ANSI_RESET] [ANSI_BOLD]@username/scratch/cmd/main.f[ANSI_RESET]
[ANSI_GRAY][..]/cmd/main/main.mbt[LINE_NUMBER][ANSI_RESET] [ANSI_CYAN]by[ANSI_RESET] [ANSI_BOLD]main[ANSI_RESET]

"#]]);
}
