use crate::util::{check, moon_bin};
use crate::{TestDir, get_stdout};
use expect_test::expect;

#[test]
fn test_moon_help() {
    let dir = TestDir::new_empty();
    check(
        get_stdout(&dir, ["help"]).replace("moon.exe", "moon"),
        expect![[r#"
            The build system and package manager for MoonBit.

            Usage: moon [OPTIONS] <COMMAND>

            Commands:
              new                    Create a new MoonBit module
              build                  Build the current package
              check                  Check the current package, but don't build object files
              prove                  Prove the current package
              run                    Run a main package
              test                   Test the current package
              testcli                Run CLI tests with the current toolchain's mooncram
              clean                  Remove local build outputs or configured global caches
              fmt                    Format source code
              doc                    Generate documentation or searching documentation for a symbol
              explain                Explain compiler diagnostics and language topics
              info                   Generate public interface (`.mbti`) files for all packages in the module or workspace
              bench                  Run benchmarks in the current package
              add                    Add a dependency
              remove                 Remove a dependency
              install                Install a binary package globally
              tree                   Display the dependency tree
              fetch                  Download a package to .repos directory (unstable)
              search                 Search modules and package summaries in the registry
              view                   View a registry module or a user's published modules
              work                   Workspace maintenance commands
              login                  Log in to your account
              whoami                 Show login status and username
              register               Register an account at mooncakes.io
              publish                Publish the current module
              package                Package the current module
              deprecate              Deprecate or restore all existing versions of a published module
              update                 Refresh registry metadata and report dependency updates
              coverage               Code coverage utilities
              generate-build-matrix  Generate build matrix for benchmarking (legacy feature)
              upgrade                Upgrade toolchains
              shell-completion       Generate shell completion for bash/elvish/fish/pwsh/zsh to stdout
              version                Print version information and exit
              help                   Print this message or the help of the given subcommand(s)

            Options:
              -V, --version  Print all version information and exit
              -h, --help     Print help

            Common Options:
              -C <DIR>
                      Change to DIR before doing anything else (must appear before the subcommand). Relative paths in other options and arguments are interpreted relative to DIR. Example: `moon -C a run .` runs the same as invoking `moon run .` from within `a`
                  --target-dir <TARGET_DIR>
                      The target directory. Defaults to `<project-root>/_build`, or `<source-dir>/_build/<file-name>` for a standalone file
              -q, --quiet
                      Suppress output
              -v, --verbose
                      Increase verbosity
                  --trace
                      Trace the execution of the program
                  --dry-run
                      Do not actually run the command
              -Z, --unstable-feature <UNSTABLE_FEATURE>
                      Unstable flags to MoonBuild [env: MOON_UNSTABLE=] [default: ]
        "#]],
    );
}

#[test]
fn test_moon_info_help_explains_target_and_default_behavior() {
    let dir = TestDir::new_empty();
    let output = get_stdout(&dir, ["help", "info"]);
    assert!(output.contains("By default, `moon info` writes `pkg.generated.mbti`"));
    assert!(output.contains("canonical backend: module `preferred-backend`, then workspace"));
    assert!(
        output.contains("`--target` inspects backend-specific interfaces and reports differences")
    );
    assert!(output.contains("Inspect one or more target backends without changing the canonical"));
}

#[test]
fn test_moon_version_flag() {
    let dir = TestDir::new_empty();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("--version")
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
moon [..]
moonc [..]
moonrun [..]
...
"#]])
        .stderr_eq("");
}

#[test]
fn test_moon_version_flag_reports_unstable_features() {
    let dir = TestDir::new_empty();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["-Z", "rr_moon_pkg", "--version"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
moon [..]
moonc [..]
moonrun [..]

Feature flags enabled: rr_moon_mod,rr_moon_pkg

"#]])
        .stderr_eq("");
}

#[test]
fn test_moon_flag_without_subcommand_shows_help() {
    let dir = TestDir::new_empty();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("-q")
        .assert()
        .code(2)
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
The build system and package manager for MoonBit.

Usage: moon [OPTIONS] <COMMAND>

Commands:
...
  version                Print version information and exit
  help                   Print this message or the help of the given subcommand(s)

Options:
  -V, --version
          Print all version information and exit
...
Common Options:
...

"#]]);
}

#[test]
fn test_moon_add_help_includes_no_update() {
    let dir = TestDir::new_empty();
    let out = get_stdout(&dir, ["add", "--help"]).replace("moon.exe", "moon");
    assert!(out.contains("--no-update"));
    assert!(out.contains("--upgrade"));
}
