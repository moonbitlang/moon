use expect_test::expect;

use crate::{TestDir, dry_run_utils::line_with, get_stdout, util::check};

#[cfg(unix)]
use crate::get_err_stderr;

#[test]
fn run_dry_run_uses_selected_profile_for_runtime_artifact() {
    let dir = TestDir::new("debug_flag_test");

    let default_run = get_stdout(&dir, ["run", "main", "--dry-run", "--nostd"]);
    assert_moonrun_line(&default_run, false);

    let release_run = get_stdout(&dir, ["run", "main", "--dry-run", "--release", "--nostd"]);
    assert_moonrun_line(&release_run, true);
}

#[cfg(unix)]
#[test]
fn profile_flags_conflict_in_cli() {
    let dir = TestDir::new("debug_flag_test");

    check(
        get_err_stderr(&dir, ["test", "--release", "--debug"]),
        expect![[r#"
            error: the argument '--release' cannot be used with '--debug'

            Usage: moon test --release [PATH]...

            For more information, try '--help'.
        "#]],
    );

    check(
        get_err_stderr(&dir, ["build", "--debug", "--release"]),
        expect![[r#"
            error: the argument '--debug' cannot be used with '--release'

            Usage: moon build --debug [PATH]...

            For more information, try '--help'.
        "#]],
    );

    check(
        get_err_stderr(&dir, ["check", "--release", "--debug"]),
        expect![[r#"
            error: the argument '--release' cannot be used with '--debug'

            Usage: moon check --release [PATH]...

            For more information, try '--help'.
        "#]],
    );

    check(
        get_err_stderr(&dir, ["run", "main", "--debug", "--release"]),
        expect![[r#"
            error: the argument '--debug' cannot be used with '--release'

            Usage: moon run --debug <PACKAGE_OR_MBT_FILE> [ARGS]...

            For more information, try '--help'.
        "#]],
    );
}

#[cfg(unix)]
#[test]
fn cli_reports_selector_conflicts_before_planning() {
    let dir = TestDir::new("debug_flag_test");

    let check_stderr = get_err_stderr(&dir, ["check", "-p", "lib", "--dry-run", "lib"]);
    assert!(
        check_stderr.contains("cannot be used with"),
        "stderr: {check_stderr}"
    );
    assert!(
        !check_stderr.contains("Failed to calculate build plan"),
        "stderr: {check_stderr}"
    );

    let test_stderr = get_err_stderr(&dir, ["test", "--file", "hello.mbt", "--dry-run"]);
    assert!(test_stderr.contains("--package"), "stderr: {test_stderr}");
    assert!(test_stderr.contains("required"), "stderr: {test_stderr}");
    assert!(
        !test_stderr.contains("`--file` must be used with `--package`"),
        "stderr: {test_stderr}"
    );

    let bench_stderr = get_err_stderr(&dir, ["bench", "--package", "--dry-run"]);
    assert!(bench_stderr.contains("--package"), "stderr: {bench_stderr}");
    assert!(bench_stderr.contains("value"), "stderr: {bench_stderr}");

    let test_index_stderr = get_err_stderr(
        &dir,
        [
            "test",
            "--package",
            "lib",
            "--file",
            "hello.mbt",
            "--index",
            "0",
            "--doc-index",
            "0",
        ],
    );
    assert!(
        test_index_stderr.contains("cannot be used with"),
        "stderr: {test_index_stderr}"
    );

    let embed_mode_stderr = get_err_stderr(
        &dir,
        [
            "tool", "embed", "--binary", "--text", "-i", "in.bin", "-o", "out.mbt",
        ],
    );
    assert!(
        embed_mode_stderr.contains("cannot be used with"),
        "stderr: {embed_mode_stderr}"
    );

    let build_binary_dep_stderr = get_err_stderr(
        &dir,
        [
            "tool",
            "build-binary-dep",
            "hello",
            "--all-pkgs",
            "--install-path",
            "bin",
        ],
    );
    assert!(
        build_binary_dep_stderr.contains("cannot be used with"),
        "stderr: {build_binary_dep_stderr}"
    );
}

#[test]
fn check_path_selector_smoke() {
    let dir = TestDir::new("debug_flag_test");
    let check_with_path_selector = get_stdout(&dir, ["check", "lib", "--no-mi", "--dry-run"]);
    assert!(
        check_with_path_selector.contains("moonc check"),
        "stdout: {check_with_path_selector}"
    );

    #[cfg(unix)]
    {
        let stderr = get_err_stderr(&dir, ["check", "lib", "main", "--no-mi", "--dry-run"]);
        assert!(
            stderr.contains("`--no-mi` requires the selector to resolve to a single package"),
            "stderr: {stderr}"
        );

        let stderr = get_err_stderr(&dir, ["check", "notes", "--no-mi", "--dry-run"]);
        assert!(
            stderr.contains("`--no-mi` requires the selector to resolve to a single package"),
            "stderr: {stderr}"
        );

        let stderr = get_err_stderr(
            &dir,
            ["check", "notes", "--patch-file", "patch.json", "--dry-run"],
        );
        assert!(
            stderr.contains("`--patch-file` requires the selector to resolve to a single package"),
            "stderr: {stderr}"
        );
    }
}

#[test]
fn build_explicit_empty_selection_does_not_expand_to_workspace() {
    let dir = TestDir::new("debug_flag_test");
    let stdout = get_stdout(&dir, ["build", "notes", "--target", "js", "--dry-run"]);
    assert!(!stdout.contains("moonc build-package"), "stdout: {stdout}");
}

fn assert_moonrun_line(output: &str, release: bool) {
    let empty: &[&str] = &[];
    let line = line_with(output, "moonrun", empty);
    let target_prefix = if release {
        "_build/wasm-gc/release/"
    } else {
        "_build/wasm-gc/debug/"
    };
    assert!(
        line.contains(target_prefix),
        "expected moonrun to execute artifact in `{}`, saw `{}`",
        target_prefix,
        line
    );
}
