use crate::{TestDir, get_stdout_with_envs, moon_process_cmd};

fn command_mentions_c_source(line: &str) -> bool {
    line.split_whitespace()
        .any(|arg| arg.trim_matches('\'').ends_with(".c"))
}

fn assert_debug_uses_direct_object_without_env(dir: &TestDir, args: &[&str]) {
    let output = moon_process_cmd(dir)
        .env_remove("MOONBIT_NEW_NATIVE")
        .env_remove("MOON_CC")
        .args(args)
        .output()
        .expect("failed to get debug dry-run output");
    assert!(
        output.status.success(),
        "debug dry-run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("__moonbit_link_core__"),
        "debug dry-run did not use the direct object backend:\n{stdout}"
    );
    assert!(
        !stdout
            .lines()
            .any(|line| line.contains("moonc link-core") && command_mentions_c_source(line)),
        "debug dry-run unexpectedly generated C source:\n{stdout}"
    );
}

fn assert_debug_uses_generated_c_when_disabled(dir: &TestDir, args: &[&str]) {
    let output = get_stdout_with_envs(
        dir,
        args.iter().copied(),
        [("MOONBIT_NEW_NATIVE", "0"), ("MOON_CC", "cc")],
    );

    assert!(
        output
            .lines()
            .any(|line| line.contains("moonc link-core") && command_mentions_c_source(line)),
        "disabled new native dry-run did not generate C source:\n{output}"
    );
    assert!(
        !output.contains("__moonbit_link_core__"),
        "disabled new native dry-run unexpectedly used the direct object backend:\n{output}"
    );
}

fn assert_release_uses_generated_c(dir: &TestDir, args: &[&str]) {
    let output = get_stdout_with_envs(
        dir,
        args.iter().copied(),
        [("MOONBIT_NEW_NATIVE", "1"), ("MOON_CC", "cc")],
    );

    assert!(
        output
            .lines()
            .any(|line| line.contains("moonc link-core") && command_mentions_c_source(line)),
        "release dry-run did not generate C source:\n{output}"
    );
    assert!(
        output
            .lines()
            .any(|line| line.contains(" -O2 ") && command_mentions_c_source(line)),
        "release dry-run did not compile generated C with -O2:\n{output}"
    );
    assert!(
        !output.contains("__moonbit_link_core__"),
        "release dry-run unexpectedly used the direct object backend:\n{output}"
    );
}

#[test]
fn test_new_native_is_default_for_run_and_test_e2e() {
    let dir = TestDir::new("native_backend/new_native_e2e");

    let run_output = moon_process_cmd(&dir)
        .env_remove("MOONBIT_NEW_NATIVE")
        .env_remove("MOON_CC")
        .args(["run", "main", "--target", "native"])
        .output()
        .expect("failed to run new native executable");
    assert!(
        run_output.status.success(),
        "new native run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run_output.stdout),
        String::from_utf8_lossy(&run_output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run_output.stdout),
        "new native run ok\n"
    );

    let test_output = moon_process_cmd(&dir)
        .env_remove("MOONBIT_NEW_NATIVE")
        .env_remove("MOON_CC")
        .args(["test", "--target", "native", "-v", "--no-parallelize"])
        .output()
        .expect("failed to run new native tests");
    assert!(
        test_output.status.success(),
        "new native test failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&test_output.stdout),
        String::from_utf8_lossy(&test_output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&test_output.stdout).contains("new native test ok\n"),
        "new native test stdout did not contain expected output\nstdout:\n{}",
        String::from_utf8_lossy(&test_output.stdout)
    );
}

#[test]
fn test_new_native_is_default_for_debug_build_run_and_test() {
    let dir = TestDir::new("native_backend/new_native_e2e");

    assert_debug_uses_direct_object_without_env(
        &dir,
        &["build", "--target", "native", "--dry-run"],
    );
    assert_debug_uses_direct_object_without_env(
        &dir,
        &["run", "main", "--target", "native", "--dry-run"],
    );
    assert_debug_uses_direct_object_without_env(&dir, &["test", "--target", "native", "--dry-run"]);
}

#[test]
fn test_new_native_zero_uses_generated_c_for_debug_build_run_and_test() {
    let dir = TestDir::new("native_backend/new_native_e2e");

    assert_debug_uses_generated_c_when_disabled(
        &dir,
        &["build", "--target", "native", "--dry-run"],
    );
    assert_debug_uses_generated_c_when_disabled(
        &dir,
        &["run", "main", "--target", "native", "--dry-run"],
    );
    assert_debug_uses_generated_c_when_disabled(&dir, &["test", "--target", "native", "--dry-run"]);
}

#[test]
fn test_new_native_release_uses_generated_c_with_o2() {
    let dir = TestDir::new("native_backend/new_native_e2e");

    assert_release_uses_generated_c(
        &dir,
        &["build", "--target", "native", "--release", "--dry-run"],
    );
    assert_release_uses_generated_c(
        &dir,
        &[
            "run",
            "main",
            "--target",
            "native",
            "--release",
            "--dry-run",
        ],
    );
    assert_release_uses_generated_c(
        &dir,
        &["test", "--target", "native", "--release", "--dry-run"],
    );
}
