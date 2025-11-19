use crate::TestDir;
use expect_test::expect_file;

use super::{assert_native_backend_graph, assert_native_backend_graph_no_env};

#[test]
#[cfg(unix)]
#[ignore = "platform-dependent behavior"]
fn test_native_backend_cc_flags() {
    let dir = TestDir::new("native_backend/cc_flags");
    assert_native_backend_graph_no_env(
        &dir,
        "build_native_graph.jsonl",
        &["build", "--target", "native", "--dry-run", "--sort-input"],
        expect_file!["cc_flags/build_native_graph.jsonl.snap"],
    );
    assert_native_backend_graph_no_env(
        &dir,
        "build_wasm_gc_graph.jsonl",
        &["build", "--target", "wasm-gc", "--dry-run", "--sort-input"],
        expect_file!["cc_flags/build_wasm_gc_graph.jsonl.snap"],
    );
    assert_native_backend_graph_no_env(
        &dir,
        "test_native_graph.jsonl",
        &["test", "--target", "native", "--dry-run", "--sort-input"],
        expect_file!["cc_flags/test_native_graph.jsonl.snap"],
    );
    assert_native_backend_graph_no_env(
        &dir,
        "test_wasm_graph.jsonl",
        &["test", "--target", "wasm", "--dry-run"],
        expect_file!["cc_flags/test_wasm_graph.jsonl.snap"],
    );
    assert_native_backend_graph_no_env(
        &dir,
        "run_native_graph.jsonl",
        &[
            "run",
            "main",
            "--target",
            "native",
            "--dry-run",
            "--sort-input",
        ],
        expect_file!["cc_flags/run_native_graph.jsonl.snap"],
    );
    assert_native_backend_graph_no_env(
        &dir,
        "run_wasm_graph.jsonl",
        &[
            "run",
            "main",
            "--target",
            "wasm",
            "--dry-run",
            "--sort-input",
        ],
        expect_file!["cc_flags/run_wasm_graph.jsonl.snap"],
    );
}

#[test]
#[cfg(unix)]
fn test_native_backend_cc_flags_with_env_override() {
    let dir = TestDir::new("native_backend/cc_flags");
    assert_native_backend_graph(
        &dir,
        "build_native_env_graph.jsonl",
        &["build", "--target", "native", "--dry-run", "--sort-input"],
        &[("MOON_CC", "x86_64-unknown-fake_os-fake_libc-gcc")],
        expect_file!["cc_flags/build_native_env_graph.jsonl.snap"],
    );
    assert_native_backend_graph(
        &dir,
        "test_native_env_graph.jsonl",
        &["test", "--target", "native", "--dry-run", "--sort-input"],
        &[("MOON_CC", "x86_64-unknown-fake_os-fake_libc-gcc")],
        expect_file!["cc_flags/test_native_env_graph.jsonl.snap"],
    );
    assert_native_backend_graph(
        &dir,
        "run_native_env_graph.jsonl",
        &[
            "run",
            "main",
            "--target",
            "native",
            "--dry-run",
            "--sort-input",
        ],
        &[("MOON_CC", "x86_64-unknown-fake_os-fake_libc-gcc")],
        expect_file!["cc_flags/run_native_env_graph.jsonl.snap"],
    );
    assert_native_backend_graph(
        &dir,
        "build_native_env_paths_graph.jsonl",
        &["build", "--target", "native", "--dry-run", "--sort-input"],
        &[
            (
                "MOON_CC",
                "/some/path/A/x86_64-unknown-fake_os-fake_libc-gcc",
            ),
            (
                "MOON_AR",
                "/other/path/B/x86_64-unknown-fake_os-fake_libc-ar",
            ),
        ],
        expect_file!["cc_flags/build_native_env_paths_graph.jsonl.snap"],
    );
    assert_native_backend_graph(
        &dir,
        "test_native_env_paths_graph.jsonl",
        &["test", "--target", "native", "--dry-run", "--sort-input"],
        &[
            (
                "MOON_CC",
                "/some/path/A/x86_64-unknown-fake_os-fake_libc-gcc",
            ),
            (
                "MOON_AR",
                "/other/path/B/x86_64-unknown-fake_os-fake_libc-ar",
            ),
        ],
        expect_file!["cc_flags/test_native_env_paths_graph.jsonl.snap"],
    );
    assert_native_backend_graph(
        &dir,
        "run_native_env_paths_graph.jsonl",
        &[
            "run",
            "main",
            "--target",
            "native",
            "--dry-run",
            "--sort-input",
        ],
        &[
            (
                "MOON_CC",
                "/some/path/A/x86_64-unknown-fake_os-fake_libc-gcc",
            ),
            (
                "MOON_AR",
                "/other/path/B/x86_64-unknown-fake_os-fake_libc-ar",
            ),
        ],
        expect_file!["cc_flags/run_native_env_paths_graph.jsonl.snap"],
    );
}
