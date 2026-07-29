#[cfg(windows)]
use std::process::Command;

#[cfg(not(windows))]
use crate::{TestDir, get_err_stderr_with_envs};
#[cfg(windows)]
use crate::{TestDir, get_stdout_with_envs};
#[cfg(unix)]
use expect_test::expect_file;

#[cfg(unix)]
use super::unix_graph::{assert_native_backend_graph, assert_native_backend_graph_no_env};

#[cfg(windows)]
fn link_commands_with_compiler(output: &str, compiler_path: &str) -> Vec<String> {
    let compiler_path = compiler_path.replace('\\', "/").to_ascii_lowercase();
    output
        .lines()
        .filter(|line| {
            let line = line.replace('\\', "/").to_ascii_lowercase();
            line.contains(&compiler_path) && line.contains(" -o ") && !line.contains(" -c ")
        })
        .map(str::to_string)
        .collect()
}

#[cfg(windows)]
fn detect_clang_toolchain() -> Option<(String, String)> {
    let clang_path = which::which("clang").ok()?;
    let output = Command::new(&clang_path)
        .arg("-dumpmachine")
        .output()
        .ok()?;
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
        Some((clang_path.display().to_string(), target))
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
    let fake_toolchain = dir.join("fake-toolchain");
    let fake_path = fake_toolchain.join("bin").display().to_string();
    let bare_override_env = [
        ("MOONBIT_NEW_NATIVE", "0"),
        ("MOON_CC", "x86_64-unknown-fake_os-fake_libc-gcc"),
        ("PATH", fake_path.as_str()),
    ];
    assert_native_backend_graph(
        &dir,
        "build_native_env_graph.jsonl",
        &["build", "--target", "native", "--dry-run", "--sort-input"],
        &bare_override_env,
        expect_file!["cc_flags/build_native_env_graph.jsonl.snap"],
    );
    assert_native_backend_graph(
        &dir,
        "test_native_env_graph.jsonl",
        &["test", "--target", "native", "--dry-run", "--sort-input"],
        &bare_override_env,
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
        &bare_override_env,
        expect_file!["cc_flags/run_native_env_graph.jsonl.snap"],
    );
    let compiler = fake_toolchain
        .join("A/x86_64-unknown-fake_os-fake_libc-gcc")
        .display()
        .to_string();
    let archiver = fake_toolchain
        .join("B/x86_64-unknown-fake_os-fake_libc-ar")
        .display()
        .to_string();
    let path_override_env = [
        ("MOONBIT_NEW_NATIVE", "0"),
        ("MOON_CC", compiler.as_str()),
        ("MOON_AR", archiver.as_str()),
    ];
    assert_native_backend_graph(
        &dir,
        "build_native_env_paths_graph.jsonl",
        &["build", "--target", "native", "--dry-run", "--sort-input"],
        &path_override_env,
        expect_file!["cc_flags/build_native_env_paths_graph.jsonl.snap"],
    );
    assert_native_backend_graph(
        &dir,
        "test_native_env_paths_graph.jsonl",
        &["test", "--target", "native", "--dry-run", "--sort-input"],
        &path_override_env,
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
        &path_override_env,
        expect_file!["cc_flags/run_native_env_paths_graph.jsonl.snap"],
    );
}

#[test]
#[cfg(unix)]
fn test_native_backend_reports_missing_tool_role_during_resolution() {
    let dir = TestDir::new("native_backend/cc_flags");
    let fake_bin = dir.join("fake-toolchain/bin");
    let fake_path = fake_bin.display().to_string();
    let empty_path = dir.join("fake-toolchain/empty").display().to_string();
    let args = ["build", "--target", "native", "--dry-run"];

    let compiler_error = get_err_stderr_with_envs(
        &dir,
        args,
        [("MOON_CC", "missing-gcc"), ("PATH", empty_path.as_str())],
    );
    assert!(
        compiler_error.contains("native compiler executable `missing-gcc`"),
        "unexpected compiler resolution error: {compiler_error}"
    );

    let archiver_error = get_err_stderr_with_envs(
        &dir,
        args,
        [("MOON_CC", "fake-gcc"), ("PATH", fake_path.as_str())],
    );
    assert!(
        archiver_error.contains("native archiver executable `fake-ar`"),
        "unexpected archiver resolution error: {archiver_error}"
    );
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn test_native_backend_new_native_with_env_override() {
    let dir = TestDir::new("native_backend/cc_flags");
    let fake_toolchain = dir.join("fake-toolchain");
    let fake_path = fake_toolchain.join("bin").display().to_string();
    let envs = &[
        ("MOONBIT_NEW_NATIVE", "1"),
        ("MOON_CC", "x86_64-unknown-fake_os-fake_libc-gcc"),
        ("PATH", fake_path.as_str()),
    ];
    assert_native_backend_graph(
        &dir,
        "build_native_new_native_env_graph.jsonl",
        &["build", "--target", "native", "--dry-run", "--sort-input"],
        envs,
        expect_file!["cc_flags/build_native_new_native_env_graph.jsonl.snap"],
    );
    assert_native_backend_graph(
        &dir,
        "test_native_new_native_env_graph.jsonl",
        &["test", "--target", "native", "--dry-run", "--sort-input"],
        envs,
        expect_file!["cc_flags/test_native_new_native_env_graph.jsonl.snap"],
    );
    assert_native_backend_graph(
        &dir,
        "run_native_new_native_env_graph.jsonl",
        &[
            "run",
            "main",
            "--target",
            "native",
            "--dry-run",
            "--sort-input",
        ],
        envs,
        expect_file!["cc_flags/run_native_new_native_env_graph.jsonl.snap"],
    );
}

#[test]
#[cfg(windows)]
fn test_native_backend_clang_uses_target_specific_libm_behavior() {
    let in_ci = std::env::var("CI").is_ok();
    let Some((clang_path, target)) = detect_clang_toolchain() else {
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

    let link_lines = link_commands_with_compiler(&output, &clang_path);
    assert!(
        !link_lines.is_empty(),
        "expected at least one link command using resolved clang path `{clang_path}`:\n{output}"
    );
    if target.contains("msvc") {
        assert!(
            link_lines.iter().all(|line| !line.contains(" -lm")),
            "unexpected -lm for clang target `{target}`:\n{}",
            link_lines.join("\n")
        );
        assert!(
            !output.contains("MOONBIT_USE_SIMDUTF") && !output.contains("moonbit_simdutf.o"),
            "unexpected simdutf use for Windows clang target `{target}`:\n{output}",
        );
    } else {
        assert!(
            link_lines.iter().any(|line| line.contains(" -lm")),
            "expected -lm for clang target `{target}`:\n{}",
            link_lines.join("\n")
        );
    }
}
