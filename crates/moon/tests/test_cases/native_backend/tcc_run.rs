use crate::TestDir;
use expect_test::expect_file;

use super::assert_native_backend_graph_no_env;
#[cfg(windows)]
use crate::get_stdout_with_envs;

#[test]
#[cfg(unix)]
fn test_native_backend_tcc_run() {
    let dir = TestDir::new("native_backend/tcc_run");
    assert_native_backend_graph_no_env(
        &dir,
        "build_native_graph.jsonl",
        &["build", "--target", "native", "--dry-run", "--sort-input"],
        expect_file!["tcc_run/build_native_graph.jsonl.snap"],
    );

    assert_native_backend_graph_no_env(
        &dir,
        "test_native_linux_graph.jsonl",
        &["test", "--target", "native", "--dry-run", "--sort-input"],
        expect_file!["tcc_run/test_native_linux_graph.jsonl.snap"],
    );
}

#[test]
#[cfg(windows)]
fn test_native_backend_tcc_run_windows_disabled_by_default() {
    let dir = TestDir::new("native_backend/tcc_run");
    let out = get_stdout_with_envs(
        &dir,
        ["test", "--target", "native", "--dry-run", "--sort-input"],
        [] as [(&str, &str); 0],
    );
    assert!(
        !out.contains("write-tcc-rsp-file"),
        "unexpected tcc-run graph on Windows without opt-in:\n{out}"
    );
}

#[test]
#[cfg(windows)]
fn test_native_backend_tcc_run_windows_experimental() {
    let dir = TestDir::new("native_backend/tcc_run");
    let out = get_stdout_with_envs(
        &dir,
        ["test", "--target", "native", "--dry-run", "--sort-input"],
        [("MOON_ENABLE_WINDOWS_TCC_RUN", "1")],
    );
    assert!(
        out.contains("write-tcc-rsp-file"),
        "expected tcc-run graph on Windows experimental path:\n{out}"
    );
}

#[test]
#[cfg(windows)]
fn test_native_backend_tcc_run_windows_with_env_tcc_cc_uses_msvc_runtime() {
    let dir = TestDir::new("native_backend/tcc_run");
    let out = get_stdout_with_envs(
        &dir,
        ["test", "--target", "native", "--dry-run", "--sort-input"],
        [
            ("MOON_ENABLE_WINDOWS_TCC_RUN", "1"),
            ("MOON_CC", "x86_64-unknown-fake_os-fake_libc-tcc"),
        ],
    );
    assert!(
        out.contains("write-tcc-rsp-file"),
        "expected tcc-run graph on Windows with MOON_CC=tcc:\n{out}"
    );
    assert!(
        !out.contains("-lm"),
        "unexpected math library linkage for Windows tcc msvc-runtime path:\n{out}"
    );
    assert!(
        out.contains("libruntime.lib"),
        "expected msvc-style runtime import lib for Windows tcc path:\n{out}"
    );
}
