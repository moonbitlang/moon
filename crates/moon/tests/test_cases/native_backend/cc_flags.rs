#[cfg(windows)]
use std::process::Command;

use crate::TestDir;
#[cfg(windows)]
use crate::get_stdout_with_envs;
use expect_test::expect_file;

use super::{assert_native_backend_graph, assert_native_backend_graph_no_env};

#[cfg(windows)]
fn link_commands_with_compiler(output: &str, compiler_name: &str) -> Vec<String> {
    output
        .lines()
        .filter(|line| {
            line.contains(compiler_name) && line.contains(" -o ") && !line.contains(" -c ")
        })
        .map(str::to_string)
        .collect()
}

#[cfg(windows)]
fn detect_clang_target_triple() -> Option<String> {
    let clang_path = which::which("clang").ok()?;
    let output = Command::new(clang_path).arg("-dumpmachine").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let target = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if target.is_empty() {
        None
    } else {
        Some(target)
    }
}

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

#[test]
#[cfg(windows)]
fn test_native_backend_clang_uses_target_specific_libm_behavior() {
    let in_ci = std::env::var("CI").is_ok();
    let Some(target) = detect_clang_target_triple() else {
        if in_ci {
            panic!("clang -dumpmachine is unavailable on Windows CI");
        }
        eprintln!("skipping native clang test: clang -dumpmachine is unavailable");
        return;
    };
    if in_ci {
        assert!(
            target.contains("msvc"),
            "expected clang target to be msvc on Windows CI, got `{target}`"
        );
    }

    let dir = TestDir::new("native_backend/cc_flags");
    let output = get_stdout_with_envs(
        &dir,
        ["build", "--target", "native", "--dry-run", "--sort-input"],
        [("MOON_CC", "clang")],
    );

    let link_lines = link_commands_with_compiler(&output, "clang ");
    assert!(
        !link_lines.is_empty(),
        "expected at least one link command using clang"
    );
    if target.contains("msvc") {
        assert!(
            link_lines.iter().all(|line| !line.contains(" -lm")),
            "unexpected -lm for clang target `{target}`:\n{}",
            link_lines.join("\n")
        );
    } else {
        assert!(
            link_lines.iter().any(|line| line.contains(" -lm")),
            "expected -lm for clang target `{target}`:\n{}",
            link_lines.join("\n")
        );
    }
}
