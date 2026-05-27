use crate::{TestDir, moon_process_cmd};

#[test]
fn test_new_native_run_and_test_e2e() {
    let dir = TestDir::new("native_backend/new_native_e2e");

    let run_output = moon_process_cmd(&dir)
        .env("MOONBIT_NEW_NATIVE", "1")
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
        .env("MOONBIT_NEW_NATIVE", "1")
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
