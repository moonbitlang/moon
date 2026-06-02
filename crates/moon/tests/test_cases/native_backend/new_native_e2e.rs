use crate::{TestDir, moon_process_cmd};

#[test]
fn test_new_native_run_and_test_e2e() {
    let dir = TestDir::new("native_backend/new_native_e2e");
    let runner = native_runner_for_tests();

    let run_output = moon_process_cmd(&dir)
        .env("MOONBIT_NEW_NATIVE", "1")
        .env("MOON_NATIVE_RUNNER_OVERRIDE", &runner)
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
    assert!(
        dir.join("_build/native/debug/build/main/main.dylib")
            .is_file(),
        "moon run should build a native dylib"
    );

    let test_output = moon_process_cmd(&dir)
        .env("MOONBIT_NEW_NATIVE", "1")
        .env("MOON_NATIVE_RUNNER_OVERRIDE", &runner)
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
    assert!(
        dir.join("_build/native/debug/test/main/main.internal_test.dylib")
            .is_file(),
        "moon test should build native test dylibs"
    );

    let build_output = moon_process_cmd(&dir)
        .env("MOONBIT_NEW_NATIVE", "1")
        .env("MOON_NATIVE_RUNNER_OVERRIDE", &runner)
        .env_remove("MOON_CC")
        .args(["build", "--target", "native"])
        .output()
        .expect("failed to build new native artifacts");
    assert!(
        build_output.status.success(),
        "new native build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build_output.stdout),
        String::from_utf8_lossy(&build_output.stderr)
    );
    assert!(
        dir.join("_build/native/debug/build/main/main.exe")
            .is_file(),
        "moon build should still produce a native executable"
    );
    assert!(
        dir.join("_build/native/debug/build/main/main.dylib")
            .is_file(),
        "moon build should also produce a native dylib"
    );
}

fn native_runner_for_tests() -> std::path::PathBuf {
    let current_exe = std::env::current_exe().expect("failed to get current test executable");
    let runner = current_exe
        .parent()
        .and_then(|path| path.parent())
        .expect("test executable should be under <target>/debug/deps")
        .join("moon-native-runner");
    if runner.exists() {
        return runner;
    }

    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("moon crate should be under <workspace>/crates/moon");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = std::process::Command::new(cargo)
        .current_dir(workspace_root)
        .args(["build", "-p", "moon-native-runner"])
        .status()
        .expect("failed to build moon-native-runner for native e2e tests");
    assert!(
        status.success(),
        "failed to build moon-native-runner for native e2e tests"
    );
    runner
}
