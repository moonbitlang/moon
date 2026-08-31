use crate::{TestDir, moon_process_cmd};
use expect_test::expect_file;

use super::unix_graph::{assert_native_backend_graph, prepend_to_path};

#[test]
fn test_native_backend_system_cc_fallback() {
    let dir = TestDir::new("native_backend/tcc_run");
    let fake_bin = dir.join("fake-toolchain/bin");
    let fake_path = prepend_to_path(&fake_bin);
    let envs = &[("MOONBIT_NEW_NATIVE", "0"), ("PATH", fake_path.as_str())];
    assert_native_backend_graph(
        &dir,
        "build_native_graph.jsonl",
        &["build", "--target", "native", "--dry-run", "--sort-input"],
        envs,
        expect_file!["tcc_run/build_native_graph.jsonl.snap"],
    );

    assert_native_backend_graph(
        &dir,
        "test_native_graph.jsonl",
        &["test", "--target", "native", "--dry-run", "--sort-input"],
        envs,
        if cfg!(target_os = "macos") {
            expect_file!["tcc_run/test_native_macos_graph.jsonl.snap"]
        } else {
            expect_file!["tcc_run/test_native_linux_graph.jsonl.snap"]
        },
    );
}

#[test]
fn test_native_system_cc_when_moon_spawned_from_other_dir() {
    let dir = TestDir::new("workspace_basic.in");
    let spawn_dir = dir.join("spawn");
    std::fs::create_dir(&spawn_dir).expect("failed to create spawn directory");

    let output = moon_process_cmd(&spawn_dir)
        .env_remove("MOON_CC")
        .env("MOONBIT_NEW_NATIVE", "0")
        .args([
            "-C",
            "..",
            "test",
            "--target",
            "native",
            "--no-parallelize",
            "--sort-input",
        ])
        .output()
        .expect("failed to run native tests with the system C compiler");
    assert!(
        output.status.success(),
        "native system-CC test failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Total tests: 2, passed: 2, failed: 0."),
        "native system-CC test stdout did not contain expected summary\nstdout:\n{stdout}",
    );
}
