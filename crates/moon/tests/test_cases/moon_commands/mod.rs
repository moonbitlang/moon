use super::*;

#[test]
fn test_moon_cmd() {
    let dir = TestDir::new("moon_commands");
    check(
        get_stdout(&dir, ["build", "--dry-run", "--nostd", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/list/lib.mbt -o ./_build/wasm-gc/debug/build/lib/list/list.core -pkg design/lib/list -pkg-sources design/lib/list:./lib/list -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./lib/queue/lib.mbt -o ./_build/wasm-gc/debug/build/lib/queue/queue.core -pkg design/lib/queue -i ./_build/wasm-gc/debug/build/lib/list/list.mi:list -pkg-sources design/lib/queue:./lib/queue -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main2/main.mbt -o ./_build/wasm-gc/debug/build/main2/main2.core -pkg design/main2 -is-main -i ./_build/wasm-gc/debug/build/lib/queue/queue.mi:queue -pkg-sources design/main2:./main2 -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/debug/build/lib/list/list.core ./_build/wasm-gc/debug/build/lib/queue/queue.core ./_build/wasm-gc/debug/build/main2/main2.core -main design/main2 -o ./_build/wasm-gc/debug/build/main2/main2.wasm -pkg-config-path ./main2/moon.pkg.json -pkg-sources design/lib/list:./lib/list -pkg-sources design/lib/queue:./lib/queue -pkg-sources design/main2:./main2 -target wasm-gc -g -O0 -source-map
            moonc build-package ./main1/main.mbt -o ./_build/wasm-gc/debug/build/main1/main1.core -pkg design/main1 -is-main -i ./_build/wasm-gc/debug/build/lib/queue/queue.mi:queue -pkg-sources design/main1:./main1 -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/debug/build/lib/list/list.core ./_build/wasm-gc/debug/build/lib/queue/queue.core ./_build/wasm-gc/debug/build/main1/main1.core -main design/main1 -o ./_build/wasm-gc/debug/build/main1/main1.wasm -pkg-config-path ./main1/moon.pkg.json -pkg-sources design/lib/list:./lib/list -pkg-sources design/lib/queue:./lib/queue -pkg-sources design/main1:./main1 -target wasm-gc -g -O0 -source-map
        "#]],
    );
}

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
              clean                  Remove the _build directory
              fmt                    Format source code
              doc                    Generate documentation or searching documentation for a symbol
              explain                Explain diagnostics from the compiler
              info                   Generate public interface (`.mbti`) files for all packages in the module or workspace
              bench                  Run benchmarks in the current package
              add                    Add a dependency
              remove                 Remove a dependency
              install                Install a binary package globally or install project dependencies (deprecated without args)
              tree                   Display the dependency tree
              fetch                  Download a package to .repos directory (unstable)
              work                   Workspace maintenance commands
              login                  Log in to your account
              whoami                 Show login status and username
              register               Register an account at mooncakes.io
              publish                Publish the current module
              package                Package the current module
              update                 Update the package registry index
              coverage               Code coverage utilities
              generate-build-matrix  Generate build matrix for benchmarking (legacy feature)
              upgrade                Upgrade toolchains
              shell-completion       Generate shell completion for bash/elvish/fish/pwsh/zsh to stdout
              version                Print version information and exit
              help                   Print this message or the help of the given subcommand(s)

            Options:
              -h, --help  Print help

            Common Options:
              -C <DIR>
                      Change to DIR before doing anything else (must appear before the subcommand). Relative paths in other options and arguments are interpreted relative to DIR. Example: `moon -C a run .` runs the same as invoking `moon run .` from within `a`
                  --target-dir <TARGET_DIR>
                      The target directory. Defaults to `<project-root>/_build`
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
    assert!(output.contains("`--target` inspects backend-specific interfaces and reports differences"));
    assert!(output.contains("Inspect one or more target backends without changing the canonical"));
}

#[test]
fn test_moon_explain_diagnostics_lists_warnings() {
    let dir = TestDir::new_empty();
    let output = get_stdout(&dir, ["explain", "--diagnostics"]);
    assert!(output.starts_with("Available warnings: \n"));
    assert!(output.contains("unused_value               Unused variable or function."));
    assert!(output.contains("partial_match              Partial pattern matching."));
}

#[test]
fn test_moon_explain_diagnostics_number_uses_integrated_docs() {
    let dir = TestDir::new_empty();
    let output = get_stdout(&dir, ["explain", "--diagnostics", "2"]);
    assert!(output.starts_with("# E0002\n"));
    assert!(output.contains("Warning name: `unused_value`"));
    assert!(output.contains("Unused variable."));
}

#[test]
fn test_moon_explain_diagnostics_mnemonic_uses_integrated_docs() {
    let dir = TestDir::new_empty();
    let output = get_stdout(&dir, ["explain", "--diagnostics", "unused_value"]);
    assert!(output.contains("# E0001"));
    assert!(output.contains("# E0002"));
}

#[test]
fn test_moon_explain_without_flags_shows_guidance() {
    let dir = TestDir::new_empty();
    check(
        get_err_stderr(&dir, ["explain"]),
        expect![[r#"
            Explain diagnostics from the compiler

            Usage: moon explain [OPTIONS]

            Options:
                  --diagnostics [<ID_OR_MNEMONIC>]  Explain diagnostics. Without a query, list warning mnemonics and IDs from `moonc`
              -h, --help                            Print help

            Common Options:
                  --target-dir <TARGET_DIR>  The target directory. Defaults to `<project-root>/_build`
              -q, --quiet                    Suppress output
              -v, --verbose                  Increase verbosity
                  --trace                    Trace the execution of the program
                  --dry-run                  Do not actually run the command

            Resources:
                Docs: https://docs.moonbitlang.com
                Skills: https://github.com/moonbitlang/skills

                Use `moon explain --diagnostics` to list warning mnemonics and IDs.
        "#]],
    );
}

#[test]
fn test_moon_doc_query_warns_and_succeeds() {
    let dir = TestDir::new_empty();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["doc", "@json"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
package "moonbitlang/core/json"
...
"#]])
        .stderr_eq(
            "Warning: `moon doc <SYMBOL>` is deprecated; use `moon ide doc <SYMBOL>` instead.\n",
        );
}

#[test]
fn test_moon_whoami_not_logged_in() {
    let dir = TestDir::new_empty();
    let moon_home = dir.join("moon_home");
    std::fs::create_dir_all(&moon_home).unwrap();
    check(
        get_stdout_with_envs(
            &dir,
            ["whoami"],
            [("MOON_HOME", moon_home.to_string_lossy().into_owned())],
        ),
        expect![[r#"
            Not logged in
        "#]],
    );
}

#[test]
fn test_moon_whoami_logged_in() {
    let dir = TestDir::new_empty();
    let moon_home = dir.join("moon_home");
    std::fs::create_dir_all(&moon_home).unwrap();
    std::fs::write(
        moon_home.join("credentials.json"),
        r#"{
  "token": "test-token",
  "username": "moonbit-user"
}
"#,
    )
    .unwrap();
    check(
        get_stdout_with_envs(
            &dir,
            ["whoami"],
            [("MOON_HOME", moon_home.to_string_lossy().into_owned())],
        ),
        expect![[r#"
            Logged in as moonbit-user
        "#]],
    );
}

#[test]
fn test_moon_whoami_without_username_suggests_relogin() {
    let dir = TestDir::new_empty();
    let moon_home = dir.join("moon_home");
    std::fs::create_dir_all(&moon_home).unwrap();
    std::fs::write(
        moon_home.join("credentials.json"),
        r#"{
  "token": "test-token"
}
"#,
    )
    .unwrap();
    check(
        get_stdout_with_envs(
            &dir,
            ["whoami"],
            [("MOON_HOME", moon_home.to_string_lossy().into_owned())],
        ),
        expect![[r#"
            Logged in, but username is unavailable. Please run `moon login` again.
        "#]],
    );
}

#[test]
fn test_moon_stdin_without_args_fails() {
    let dir = TestDir::new_empty();
    let out = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .stdin(
            r#"fn main {
  println("hello from piped stdin")
}
"#,
        )
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();
    let stderr = String::from_utf8_lossy(&out).replace("moon.exe", "moon");
    assert!(stderr.contains("Usage: moon [OPTIONS] <COMMAND>"));
}

#[test]
fn test_moon_dash_shows_run_stdin_guidance() {
    let dir = TestDir::new_empty();
    let out = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("-")
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();
    let stderr = String::from_utf8_lossy(&out);
    assert!(stderr.contains("moon run -"));
    assert!(stderr.contains(".mbtx"));
}

#[test]
fn test_moon_run_dash_reads_stdin() {
    let dir = TestDir::new_empty();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("run")
        .arg("-")
        .stdin(
            r#"fn main {
  println("hello from run dash stdin")
}
"#,
        )
        .assert()
        .success()
        .stdout_eq("hello from run dash stdin\n");
}

#[test]
fn test_moon_run_command_string_reads_inline_script() {
    let dir = TestDir::new_empty();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("run")
        .arg("-c")
        .arg(
            r#"fn main {
  println("hello from run -c")
}
"#,
        )
        .assert()
        .success()
        .stdout_eq("hello from run -c\n");
}

#[test]
fn test_moon_run_command_string_short_alias_e_reads_inline_script() {
    let dir = TestDir::new_empty();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("run")
        .arg("-e")
        .arg(
            r#"fn main {
  println("hello from run -e")
}
"#,
        )
        .assert()
        .success()
        .stdout_eq("hello from run -e\n");
}

#[test]
fn test_moon_run_command_string_conflicts_with_other_inputs() {
    let dir = TestDir::new_empty();

    let dash_stderr = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("run")
        .arg("-c")
        .arg(r#"fn main { println("hello") }"#)
        .arg("-")
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();
    let dash_stderr = String::from_utf8_lossy(&dash_stderr);
    assert!(dash_stderr.contains("cannot be used with"));
    assert!(dash_stderr.contains("-c"));

    let path_stderr = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("run")
        .arg("-c")
        .arg(r#"fn main { println("hello") }"#)
        .arg("main")
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();
    let path_stderr = String::from_utf8_lossy(&path_stderr);
    assert!(path_stderr.contains("cannot be used with"));
    assert!(path_stderr.contains("-c"));
}

#[test]
fn test_moon_run_dash_reads_stdin_with_common_flags() {
    let dir = TestDir::new_empty();
    let subdir = dir.join("subdir");
    std::fs::create_dir_all(&subdir).unwrap();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("-C")
        .arg(&subdir)
        .arg("run")
        .arg("-")
        .stdin(
            r#"fn main {
  println("hello from run dash with -C")
}
"#,
        )
        .assert()
        .success()
        .stdout_eq("hello from run dash with -C\n");
}

#[test]
fn test_moon_run_dash_reads_stdin_with_build_flags() {
    let dir = TestDir::new_empty();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("run")
        .arg("--release")
        .arg("-")
        .stdin(
            r#"fn main {
  println("hello from run dash with --release")
}
"#,
        )
        .assert()
        .success()
        .stdout_eq("hello from run dash with --release\n");
}

#[cfg(unix)]
#[test]
fn test_moon_run_dash_with_heredoc() {
    let dir = TestDir::new_empty();
    let moon = moon_bin().to_string_lossy().replace('\'', "'\\''");
    let script = format!(
        r#"MOON_BIN='{moon}'
"$MOON_BIN" run - <<'EOF'
fn main {{
  println("hello from run heredoc")
}}
EOF
"#
    );
    let output = std::process::Command::new("sh")
        .current_dir(&dir)
        .arg("-c")
        .arg(script)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "status: {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hello from run heredoc"));
}

#[cfg(unix)]
#[test]
fn test_moon_run_pipe_input() {
    let dir = TestDir::new_empty();
    let moon = moon_bin().to_string_lossy().replace('\'', "'\\''");
    let script = format!(
        r#"MOON_BIN='{moon}'
echo 'import {{
  "moonbitlang/core/list",
}}
fn main {{
  let xs : @list.List[Int] = @list.of([1, 2, 3])
  println("hello from run pipe \{{xs}}")
}}' | "$MOON_BIN" run -
"#
    );
    let output = std::process::Command::new("sh")
        .current_dir(&dir)
        .arg("-c")
        .arg(script)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "status: {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hello from run pipe"));
}

#[test]
fn test_moon_tool_demangle() {
    let dir = TestDir::new_empty();
    check(
        get_stdout(
            &dir,
            [
                "tool",
                "demangle",
                "_M0FP38username5hello4lib05hello",
                "_M0Lm7$foo.fnS12",
                "plain",
            ],
        ),
        expect![[r#"
            @username/hello/lib0.hello
            foo/12
            plain
        "#]],
    );
}

#[test]
fn test_moon_add_help_includes_no_update() {
    let dir = TestDir::new_empty();
    let out = get_stdout(&dir, ["add", "--help"]).replace("moon.exe", "moon");
    assert!(out.contains("--no-update"));
}

#[test]
fn test_manifest_path_deprecation_warning_for_non_run_commands() {
    let dir = TestDir::new("moon_commands");
    let stderr = get_stderr(
        &dir,
        ["check", "--manifest-path", "moon.mod.json", "--dry-run"],
    );
    assert!(
        stderr.contains("`--manifest-path` is deprecated"),
        "expected deprecation warning, got:\n{stderr}"
    );
}

#[test]
fn test_run_rejects_manifest_path() {
    let dir = TestDir::new("moon_commands");
    let stderr = get_err_stderr(&dir, ["run", "--manifest-path", "moon.mod.json", "main1"]);
    assert!(
        stderr.contains("`--manifest-path` is no longer supported for `moon run`"),
        "expected moon run manifest-path rejection, got:\n{stderr}"
    );
    assert!(
        stderr.contains("Use `moon -C <project-dir> run ...` instead"),
        "expected moon run manifest-path guidance, got:\n{stderr}"
    );
}

#[test]
#[ignore]
#[cfg(unix)]
fn test_bench4() {
    let dir = TestDir::new_empty();
    get_stdout(&dir, ["generate-build-matrix", "-n", "4", "-o", "bench4"]);
    check(
        get_stdout(
            &dir,
            [
                "-C",
                "./bench4",
                "run",
                "--target-dir",
                "./bench4/target",
                "main",
            ],
        ),
        expect![[r#"
            ok
        "#]],
    );

    get_stdout(
        &dir,
        [
            "-C",
            "./bench4",
            "run",
            "--target-dir",
            "./bench4/target",
            "--trace",
            "main",
        ],
    );

    let trace_file = dunce::canonicalize(dir.join("./trace.json")).unwrap();
    let t = std::fs::read_to_string(trace_file).unwrap();
    assert!(t.contains("moonbit::build::read"));
    assert!(t.contains(r#""name":"work.run""#));
    assert!(t.contains(r#""name":"run""#));
    assert!(t.contains(r#""name":"main""#));
}
