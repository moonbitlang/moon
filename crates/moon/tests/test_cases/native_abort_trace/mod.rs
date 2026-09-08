use super::*;

#[test]
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
fn test_generated_c_native_run_preserves_abort_trace() {
    let dir = TestDir::new("native_abort_trace/native_abort_trace.in");
    snapbox::cmd::Command::new(moon_bin())
        .with_assert(snapbox::Assert::new().redact_with(
            moon_test_util::stack_trace::stack_trace_redactions(dir.as_ref()),
        ))
        .current_dir(&dir)
        .env("MOONBIT_NEW_NATIVE", "0")
        .args(["run", "--target", "native", "cmd/main"])
        .assert()
        .code(128 + libc::SIGABRT)
        .stdout_eq("Hello\n")
        // MoonBit source locations are stable; runtime frames after main vary by platform.
        .stderr_eq(snapbox::str![[r#"
PanicError
    at @moonbitlang/core/option.Option::unwrap[Int] ([CORE_PATH]/builtin/option.mbt:[..])
    at @username/scratch/cmd/main.g ([..]cmd/main/main.mbt:14)
    at @username/scratch/cmd/main.f ([..]cmd/main/main.mbt:9)
    at main ([..]cmd/main/main.mbt:4)
...

"#]]);
}

#[test]
fn test_native_abort_trace() {
    let dir = TestDir::new("native_abort_trace/native_abort_trace.in");
    let redactions = moon_test_util::stack_trace::stack_trace_redactions(dir.as_ref());
    // The new native runtime aborts after reporting the panic, while the legacy
    // runtime reports a runtime error and exits 255.
    let exits_via_signal = cfg!(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ));
    let expected_stderr = if exits_via_signal {
        snapbox::str![[r#"
PanicError
    at [..]Option::unwrap[..]Int[..] ([CORE_PATH]/builtin/option.mbt:[..])
    at [..]username/scratch/cmd/main.g ([..]/cmd/main/main.mbt:[..])
    at [..]username/scratch/cmd/main.f ([..]/cmd/main/main.mbt:[..])
    at moonbit_main ([..]/cmd/main/main.mbt:[..])
    at main ([..]/cmd/main/main.mbt:[..])
...

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
    let assert = snapbox::cmd::Command::new(moon_bin())
        .with_assert(snapbox::Assert::new().redact_with(redactions))
        .current_dir(&dir)
        .env_remove("MOONBIT_NEW_NATIVE")
        .args(["run", "--target", "native", "cmd/main"])
        .assert();
    let expected_code = if exits_via_signal {
        128 + libc::SIGABRT
    } else {
        255
    };
    assert
        .code(expected_code)
        .stdout_eq("Hello\n")
        .stderr_eq(expected_stderr);
}
