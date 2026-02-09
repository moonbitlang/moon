use expect_test::expect;

use crate::{
    TestDir,
    dry_run_utils::{command_tokens, line_with},
    get_stdout,
    util::{check, moon_bin},
};

#[cfg(unix)]
use crate::get_err_stderr;

#[test]
fn debug_flag_test() {
    let dir = TestDir::new("debug_flag_test");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();

    let check_release = get_stdout(&dir, ["check", "--dry-run", "--nostd"]);
    let check_debug = get_stdout(&dir, ["check", "--dry-run", "--debug", "--nostd"]);
    // Release dry-run `check` keeps release artifact layout.
    assert_moonc_line(
        &check_release,
        "moonc check",
        &["./lib/hello.mbt"],
        true,
        None,
    );
    // Release dry-run `check` for the binary still uses release artifacts.
    assert_moonc_line(
        &check_release,
        "moonc check",
        &["./main/main.mbt"],
        true,
        None,
    );
    // Debug flag switches check artifacts to the debug directory without adding compiler flags.
    assert_moonc_line(
        &check_debug,
        "moonc check",
        &["./lib/hello.mbt"],
        false,
        Some(false),
    );
    // Same expectation for the main package check invocation.
    assert_moonc_line(
        &check_debug,
        "moonc check",
        &["./main/main.mbt"],
        false,
        Some(false),
    );

    let build_default = get_stdout(&dir, ["build", "--dry-run", "--nostd"]);
    let build_release = get_stdout(&dir, ["build", "--dry-run", "--release", "--nostd"]);
    let build_debug = get_stdout(&dir, ["build", "--dry-run", "--debug", "--nostd"]);
    // Default build uses debug artifact paths for the library.
    assert_moonc_line(
        &build_default,
        "moonc build-package",
        &["./lib/hello.mbt"],
        false,
        None,
    );
    // Default build for the main package also uses debug paths.
    assert_moonc_line(
        &build_default,
        "moonc build-package",
        &["./main/main.mbt"],
        false,
        None,
    );
    // Default build link step targets the debug artifact.
    assert_moonc_line(
        &build_default,
        "moonc link-core",
        &["-main", "hello/main"],
        false,
        None,
    );
    // Release build keeps release-only artifact paths for the library.
    assert_moonc_line(
        &build_release,
        "moonc build-package",
        &["./lib/hello.mbt"],
        true,
        None,
    );
    // Release build for the main package also uses release paths.
    assert_moonc_line(
        &build_release,
        "moonc build-package",
        &["./main/main.mbt"],
        true,
        None,
    );
    // Release build link step targets the release artifact.
    assert_moonc_line(
        &build_release,
        "moonc link-core",
        &["-main", "hello/main"],
        true,
        None,
    );
    // Debug build toggles the library compilation into the debug directory with debug flags.
    assert_moonc_line(
        &build_debug,
        "moonc build-package",
        &["./lib/hello.mbt"],
        false,
        None,
    );
    // Debug build toggles the main package compilation likewise.
    assert_moonc_line(
        &build_debug,
        "moonc build-package",
        &["./main/main.mbt"],
        false,
        None,
    );
    // Debug build link step consumes debug artifacts with debug flags.
    assert_moonc_line(
        &build_debug,
        "moonc link-core",
        &["-main", "hello/main"],
        false,
        None,
    );

    let run_default = get_stdout(&dir, ["run", "main", "--dry-run", "--nostd"]);
    let run_release = get_stdout(&dir, ["run", "main", "--dry-run", "--release", "--nostd"]);
    let run_debug = get_stdout(&dir, ["run", "main", "--dry-run", "--debug", "--nostd"]);
    // Default run recompiles the library in debug mode with flags.
    assert_moonc_line(
        &run_default,
        "moonc build-package",
        &["./lib/hello.mbt"],
        false,
        None,
    );
    // Default run recompiles the main package in debug mode with flags.
    assert_moonc_line(
        &run_default,
        "moonc build-package",
        &["./main/main.mbt"],
        false,
        None,
    );
    // Default run links debug artifacts with flags.
    assert_moonc_line(
        &run_default,
        "moonc link-core",
        &["-main", "hello/main"],
        false,
        None,
    );
    // Default run executes the debug Wasm.
    assert_moonrun_line(&run_default, false);
    // Running in release mode recompiles the library using release settings.
    assert_moonc_line(
        &run_release,
        "moonc build-package",
        &["./lib/hello.mbt"],
        true,
        None,
    );
    // Running in release mode recompiles the main package using release settings.
    assert_moonc_line(
        &run_release,
        "moonc build-package",
        &["./main/main.mbt"],
        true,
        None,
    );
    // Release run links the release artifacts.
    assert_moonc_line(
        &run_release,
        "moonc link-core",
        &["-main", "hello/main"],
        true,
        None,
    );
    // Release run executes the release Wasm.
    assert_moonrun_line(&run_release, true);
    // Debug run recompiles the library in debug mode with flags.
    assert_moonc_line(
        &run_debug,
        "moonc build-package",
        &["./lib/hello.mbt"],
        false,
        None,
    );
    // Debug run recompiles the main package in debug mode with flags.
    assert_moonc_line(
        &run_debug,
        "moonc build-package",
        &["./main/main.mbt"],
        false,
        None,
    );
    // Debug run links debug artifacts with flags.
    assert_moonc_line(
        &run_debug,
        "moonc link-core",
        &["-main", "hello/main"],
        false,
        None,
    );
    // Debug run executes the debug Wasm.
    assert_moonrun_line(&run_debug, false);

    let build_target_default = get_stdout(
        &dir,
        ["build", "--target", "wasm-gc", "--dry-run", "--nostd"],
    );
    let build_target_release = get_stdout(
        &dir,
        [
            "build",
            "--target",
            "wasm-gc",
            "--dry-run",
            "--release",
            "--nostd",
        ],
    );
    let build_target_debug = get_stdout(
        &dir,
        [
            "build",
            "--dry-run",
            "--target",
            "wasm-gc",
            "--debug",
            "--nostd",
        ],
    );
    // Default target build uses debug artifacts for the library.
    assert_moonc_line(
        &build_target_default,
        "moonc build-package",
        &["./lib/hello.mbt"],
        false,
        None,
    );
    // Default target build uses debug artifacts for the main package.
    assert_moonc_line(
        &build_target_default,
        "moonc build-package",
        &["./main/main.mbt"],
        false,
        None,
    );
    // Default target build links debug outputs.
    assert_moonc_line(
        &build_target_default,
        "moonc link-core",
        &["-main", "hello/main"],
        false,
        None,
    );
    // Explicit release target keeps release artifacts for the library.
    assert_moonc_line(
        &build_target_release,
        "moonc build-package",
        &["./lib/hello.mbt"],
        true,
        None,
    );
    // Explicit release target keeps release artifacts for the main package.
    assert_moonc_line(
        &build_target_release,
        "moonc build-package",
        &["./main/main.mbt"],
        true,
        None,
    );
    // Explicit release target link references release outputs.
    assert_moonc_line(
        &build_target_release,
        "moonc link-core",
        &["-main", "hello/main"],
        true,
        None,
    );
    // Debug build with explicit target uses debug artifacts for the library.
    assert_moonc_line(
        &build_target_debug,
        "moonc build-package",
        &["./lib/hello.mbt"],
        false,
        None,
    );
    // Debug build with explicit target uses debug artifacts for the main package.
    assert_moonc_line(
        &build_target_debug,
        "moonc build-package",
        &["./main/main.mbt"],
        false,
        None,
    );
    // Debug build with explicit target links debug outputs with flags.
    assert_moonc_line(
        &build_target_debug,
        "moonc link-core",
        &["-main", "hello/main"],
        false,
        None,
    );

    let run_target_default = get_stdout(
        &dir,
        ["run", "main", "--target", "wasm-gc", "--dry-run", "--nostd"],
    );
    let run_target_release = get_stdout(
        &dir,
        [
            "run",
            "main",
            "--target",
            "wasm-gc",
            "--dry-run",
            "--release",
            "--nostd",
        ],
    );
    let run_target_debug = get_stdout(
        &dir,
        [
            "run",
            "main",
            "--target",
            "wasm-gc",
            "--dry-run",
            "--debug",
            "--nostd",
        ],
    );
    // Default run with explicit target rebuilds the library in debug mode.
    assert_moonc_line(
        &run_target_default,
        "moonc build-package",
        &["./lib/hello.mbt"],
        false,
        None,
    );
    // Default run with explicit target rebuilds the main package in debug mode.
    assert_moonc_line(
        &run_target_default,
        "moonc build-package",
        &["./main/main.mbt"],
        false,
        None,
    );
    // Default run with explicit target links debug outputs.
    assert_moonc_line(
        &run_target_default,
        "moonc link-core",
        &["-main", "hello/main"],
        false,
        None,
    );
    // Default run with explicit target executes the debug artifact.
    assert_moonrun_line(&run_target_default, false);
    // Release run with explicit target rebuilds the library in release mode.
    assert_moonc_line(
        &run_target_release,
        "moonc build-package",
        &["./lib/hello.mbt"],
        true,
        None,
    );
    // Release run with explicit target rebuilds the main package in release mode.
    assert_moonc_line(
        &run_target_release,
        "moonc build-package",
        &["./main/main.mbt"],
        true,
        None,
    );
    // Release run with explicit target links release outputs.
    assert_moonc_line(
        &run_target_release,
        "moonc link-core",
        &["-main", "hello/main"],
        true,
        None,
    );
    // Release run with explicit target executes the release artifact.
    assert_moonrun_line(&run_target_release, true);
    // Debug run with explicit target rebuilds the library with debug flags.
    assert_moonc_line(
        &run_target_debug,
        "moonc build-package",
        &["./lib/hello.mbt"],
        false,
        None,
    );
    // Debug run with explicit target rebuilds the main package with debug flags.
    assert_moonc_line(
        &run_target_debug,
        "moonc build-package",
        &["./main/main.mbt"],
        false,
        None,
    );
    // Debug run with explicit target links debug outputs.
    assert_moonc_line(
        &run_target_debug,
        "moonc link-core",
        &["-main", "hello/main"],
        false,
        None,
    );
    // Debug run with explicit target executes the debug artifact.
    assert_moonrun_line(&run_target_debug, false);

    // release should conflict with debug
    #[cfg(unix)]
    {
        check(
            get_err_stderr(&dir, ["test", "--release", "--debug"]),
            expect![[r#"
                error: the argument '--release' cannot be used with '--debug'

                Usage: moon test --release [PATH]

                For more information, try '--help'.
            "#]],
        );

        check(
            get_err_stderr(&dir, ["build", "--debug", "--release"]),
            expect![[r#"
                error: the argument '--debug' cannot be used with '--release'

                Usage: moon build --debug [PATH]

                For more information, try '--help'.
            "#]],
        );

        check(
            get_err_stderr(&dir, ["check", "--release", "--debug"]),
            expect![[r#"
                error: the argument '--release' cannot be used with '--debug'

                Usage: moon check --release [PATH]

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
}

fn assert_moonc_line(
    output: &str,
    command: &str,
    filter: &[&str],
    release: bool,
    debug_flags: Option<bool>,
) {
    let tokens = command_tokens(output, command, filter);
    assert_tokens_follow_mode(&tokens, release, command, filter, debug_flags);
}

fn assert_tokens_follow_mode(
    tokens: &[String],
    release: bool,
    command: &str,
    filter: &[&str],
    debug_flags: Option<bool>,
) {
    let target_prefix = if release {
        "./_build/wasm-gc/release/"
    } else {
        "./_build/wasm-gc/debug/"
    };

    for token in tokens {
        if token.contains("./_build/wasm-gc/") {
            assert!(
                token.contains(target_prefix),
                "expected `{}` command with filter {:?} to use `{}` artifacts, saw `{}`",
                command,
                filter,
                target_prefix,
                token
            );
        }
    }

    let has_flag = |flag: &str| tokens.iter().any(|t| t == flag);
    let expect_debug_flags = match debug_flags {
        Some(value) => value,
        None => !release,
    };

    if expect_debug_flags {
        for flag in ["-g", "-O0", "-source-map"] {
            assert!(
                has_flag(flag),
                "expected debug `{}` command with filter {:?} to include `{}`, tokens: {:?}",
                command,
                filter,
                flag,
                tokens
            );
        }
    } else {
        for flag in ["-g", "-O0", "-source-map"] {
            assert!(
                !has_flag(flag),
                "expected release `{}` command with filter {:?} to omit `{}`, tokens: {:?}",
                command,
                filter,
                flag,
                tokens
            );
        }
    }
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
