use super::*;

fn moonx_bin(bin_dir: &tempfile::TempDir) -> std::path::PathBuf {
    let moonx = bin_dir
        .path()
        .join(if cfg!(windows) { "moonx.exe" } else { "moonx" });
    let moon = moon_bin();
    std::fs::hard_link(&moon, &moonx).unwrap_or_else(|_| {
        std::fs::copy(&moon, &moonx).expect("failed to copy moon binary as moonx");
    });
    moonx
}

#[test]
fn test_moon_cmd() {
    let dir = TestDir::new("moon_commands");
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--nostd",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/list/lib.mbt -o ./_build/wasm-gc/debug/build/lib/list/list.core -pkg design/lib/list -pkg-type library -pkg-sources design/lib/list:./lib/list -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./lib/queue/lib.mbt -o ./_build/wasm-gc/debug/build/lib/queue/queue.core -pkg design/lib/queue -pkg-type library -i ./_build/wasm-gc/debug/build/lib/list/list.mi:list -pkg-sources design/lib/queue:./lib/queue -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main2/main.mbt -o ./_build/wasm-gc/debug/build/main2/main2.core -pkg design/main2 -pkg-type executable -i ./_build/wasm-gc/debug/build/lib/queue/queue.mi:queue -pkg-sources design/main2:./main2 -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/debug/build/lib/list/list.core ./_build/wasm-gc/debug/build/lib/queue/queue.core ./_build/wasm-gc/debug/build/main2/main2.core -main design/main2 -o ./_build/wasm-gc/debug/build/main2/main2.wasm -pkg-config-path ./main2/moon.pkg.json -pkg-sources design/lib/list:./lib/list -pkg-sources design/lib/queue:./lib/queue -pkg-sources design/main2:./main2 -target wasm-gc -g -O0 -source-map
            moonc build-package ./main1/main.mbt -o ./_build/wasm-gc/debug/build/main1/main1.core -pkg design/main1 -pkg-type executable -i ./_build/wasm-gc/debug/build/lib/queue/queue.mi:queue -pkg-sources design/main1:./main1 -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
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
              runwasm                Run a local package as WebAssembly or a prebuilt WebAssembly binary
              test                   Test the current package
              clean                  Remove local build outputs or configured global caches
              fmt                    Format source code
              doc                    Generate documentation or searching documentation for a symbol
              explain                Explain compiler diagnostics and language topics
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
              -V, --version  Print all version information and exit
              -h, --help     Print help

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
fn test_moonx_uses_its_own_command_line_interface() {
    let dir = TestDir::new_empty();
    let bin_dir = tempfile::TempDir::new().expect("failed to create moonx bin directory");
    let moonx = moonx_bin(&bin_dir);

    snapbox::cmd::Command::new(&moonx)
        .current_dir(&dir)
        .arg("--help")
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
Run a package from the Mooncakes registry without installing it

Usage: moonx [OPTIONS] <PACKAGE> [PROGRAM_ARGS]...

Options:
      --target <TARGET>             [default: wasm] [possible values: wasm, native]
      --experimental-policy <PATH>  Experimental moonrun policy file; only valid for wasm
  -v, --verbose                     Show progress and execution details
  -h, --help                        Print help
  -V, --version                     Print version

"#]]);
}

#[test]
fn test_moonx_runs_cached_wasm_and_forwards_everything_after_the_coordinate() {
    let dir = TestDir::new("moon_run_with_cli_args.in");
    moon_cmd(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let moon_home = tempfile::TempDir::new().expect("failed to create temp MOON_HOME");
    let cache_path = moon_home
        .path()
        .join("registry/cache/assets/moonbitlang/parser/0.3.3/cmd/moonfmt/moonfmt.wasm");
    std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
    std::fs::copy(
        dir.join("_build/wasm/debug/build/main/main.wasm"),
        &cache_path,
    )
    .unwrap();

    let bin_dir = tempfile::TempDir::new().expect("failed to create moonx bin directory");
    let moonx = moonx_bin(&bin_dir);

    snapbox::cmd::Command::new(&moonx)
        .current_dir(&dir)
        .env("MOON_HOME", moon_home.path())
        .env("MOON_TOOLCHAIN_ROOT", toolchain_root_for_tests())
        .env("MOONRUN_OVERRIDE", moonrun_bin())
        .args([
            "moonbitlang/parser/cmd/moonfmt@0.3.3",
            "--help",
            "--arg1",
            "--target",
            "native",
            "-h",
            "-V",
            "--version",
        ])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
[..]/registry/cache/assets/moonbitlang/parser/0.3.3/cmd/moonfmt/moonfmt.wasm
--help
--arg1
--target
native
-h
-V
--version

"#]])
        .stderr_eq("");

    snapbox::cmd::Command::new(&moonx)
        .current_dir(&dir)
        .env("MOON_HOME", moon_home.path())
        .env("MOON_TOOLCHAIN_ROOT", toolchain_root_for_tests())
        .env("MOONRUN_OVERRIDE", moonrun_bin())
        .args(["moonbitlang/parser/cmd/moonfmt@0.3.3", "--", "exit-7"])
        .assert()
        .code(7)
        .stdout_eq("")
        .stderr_eq("");
}

#[test]
fn test_moonx_native_output_and_cache_contract() {
    let fixture = TestDir::new("moonx_registry_native.in");
    let moon_home = tempfile::TempDir::new().expect("failed to create temp MOON_HOME");
    let runner_files = [
        "moon.mod.json",
        "src/lib/moon.pkg.json",
        "src/lib/lib.mbt",
        "src/tool/moon.pkg.json",
        "src/tool/main.mbt",
    ]
    .map(|path| (path, std::fs::read(fixture.join(path)).unwrap()));
    let (zip_path, index_path) =
        cache_registry_package(moon_home.path(), "testuser/runner", "1.2.3", &runner_files);
    cache_registry_package(
        moon_home.path(),
        "testuser/dependency",
        "1.0.0",
        &[(
            "moon.mod.json",
            br#"{"name":"testuser/dependency","version":"1.0.0"}"#.to_vec(),
        )],
    );

    let bin_dir = tempfile::TempDir::new().expect("failed to create moonx bin directory");
    let moonx = moonx_bin(&bin_dir);

    snapbox::cmd::Command::new(&moonx)
        .current_dir(&fixture)
        .env("MOON_HOME", moon_home.path())
        .env("MOON_TOOLCHAIN_ROOT", toolchain_root_for_tests())
        .args(["--target", "native", "testuser/runner@1.2.3"])
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
[..]Package `testuser/runner` not found or is not a main package (is-main: true required)

"#]]);

    snapbox::cmd::Command::new(&moonx)
        .current_dir(&fixture)
        .env("MOON_HOME", moon_home.path())
        .env("MOON_TOOLCHAIN_ROOT", toolchain_root_for_tests())
        .args(["--target", "native", "testuser/runner/lib@1.2.3"])
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
[..]Package `testuser/runner/lib` not found or is not a main package (is-main: true required)

"#]]);

    let run = || {
        snapbox::cmd::Command::new(&moonx)
            .current_dir(&fixture)
            .env("MOON_HOME", moon_home.path())
            .env("MOON_TOOLCHAIN_ROOT", toolchain_root_for_tests())
            .args([
                "--target",
                "native",
                "testuser/runner/tool@1.2.3",
                "--child-arg",
            ])
            .assert()
            .success()
            .stdout_eq("native runner\n--child-arg\n")
            .stderr_eq(snapbox::str![[r#"
...

"#]]);
    };

    let executable = moon_home
        .path()
        .join("registry/cache/assets/testuser/runner/1.2.3/tool/tool.exe");
    snapbox::cmd::Command::new(&moonx)
        .current_dir(&fixture)
        .env("MOON_HOME", moon_home.path())
        .env("MOON_TOOLCHAIN_ROOT", toolchain_root_for_tests())
        .args([
            "--verbose",
            "--target",
            "native",
            "testuser/runner/tool@1.2.3",
            "--child-arg",
        ])
        .assert()
        .success()
        .stdout_eq("native runner\n--child-arg\n")
        .stderr_eq(snapbox::str![[r#"
Using cached testuser/runner@1.2.3
Using cached testuser/dependency@1.0.0
Building `testuser/runner/tool`...
...
'$MOON_HOME/registry/cache/assets/testuser/runner/1.2.3/tool/tool[..]' --child-arg

"#]]);

    std::fs::remove_file(&executable).unwrap();
    run();
    assert!(executable.is_file());
    std::fs::remove_file(&zip_path).unwrap();
    std::fs::remove_file(&index_path).unwrap();
    run();
}

#[cfg(any(unix, windows))]
#[test]
fn test_moonx_delegates_interrupt_to_cached_native_executable() {
    let dir = TestDir::new_empty();
    let moon_home = tempfile::TempDir::new().expect("failed to create temp MOON_HOME");
    let cache_path = moon_home
        .path()
        .join("registry/cache/assets/testuser/runner/1.2.3/tool/tool.exe");
    std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();

    let fake_bin_dir = tempfile::TempDir::new().expect("failed to create fake bin dir");
    std::fs::copy(compile_signal_fixture(fake_bin_dir.path()), &cache_path)
        .expect("failed to populate cached native executable");

    let bin_dir = tempfile::TempDir::new().expect("failed to create moonx bin directory");
    let moonx = moonx_bin(&bin_dir);
    let ready_file = bin_dir.path().join("moonx-native-ready");
    let mut command = interruptible_command(&moonx);
    let mut child = command
        .current_dir(&dir)
        .env("MOON_HOME", moon_home.path())
        .args(["--target", "native", "testuser/runner/tool@1.2.3"])
        .arg(&ready_file)
        .spawn()
        .expect("failed to spawn moonx");

    wait_for_ready_file(&mut child, &ready_file);
    send_interrupt(child.id());
    let status = wait_for_child_status(&mut child);
    assert_eq!(
        status.code(),
        Some(42),
        "cached native executable did not handle interrupt itself; status was {status}"
    );
}

#[test]
#[cfg(unix)]
fn test_tool_exec_shell_applies_cwd_env_and_execs() {
    let dir = TestDir::new_empty();
    let cwd = dir.join("exec-cwd");
    std::fs::create_dir_all(&cwd).expect("failed to create exec cwd");

    moon_cmd(&dir)
        .env("MOON_EXEC_REMOVE", "remove-me")
        .args(["tool", "exec", "--cwd"])
        .arg(&cwd)
        .args([
            "--env",
            "MOON_EXEC_TEST=expected",
            "--unset-env",
            "MOON_EXEC_REMOVE",
            "--shell",
            "pwd -P > cwd.txt; printf '%s' \"$MOON_EXEC_TEST\" > env.txt; printf '%s' \"${MOON_EXEC_REMOVE-unset}\" > unset-env.txt; printf '%s' \"$PPID\" > ppid.txt",
        ])
        .assert()
        .success();

    let actual_cwd = std::fs::read_to_string(cwd.join("cwd.txt"))
        .expect("cwd marker should be written")
        .trim()
        .to_string();
    let expected_cwd = std::fs::canonicalize(&cwd)
        .expect("cwd should canonicalize")
        .display()
        .to_string();
    assert_eq!(actual_cwd, expected_cwd);
    assert_eq!(
        std::fs::read_to_string(cwd.join("env.txt")).expect("env marker should be written"),
        "expected"
    );
    assert_eq!(
        std::fs::read_to_string(cwd.join("unset-env.txt"))
            .expect("unset env marker should be written"),
        "unset"
    );

    let ppid = std::fs::read_to_string(cwd.join("ppid.txt"))
        .expect("ppid marker should be written")
        .parse::<u32>()
        .expect("ppid marker should be a process id");
    assert_eq!(ppid, std::process::id());
}

#[test]
fn test_runwasm_runs_local_package_as_wasm_and_forwards_args() {
    let dir = TestDir::new("moon_run_with_cli_args.in");
    moon_cmd(&dir)
        .args(["runwasm", "main", "--arg1", "arg2"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
[..]/_build/wasm/debug/build/main/main.wasm
--arg1
arg2

"#]]);
}

#[test]
fn test_runwasm_help_marks_policy_as_experimental() {
    let dir = TestDir::new_empty();
    let help = get_stdout(&dir, ["runwasm", "--help"]);
    assert!(
        help.contains("--experimental-policy <PATH>"),
        "expected runwasm help to expose experimental policy flag, got:\n{help}"
    );
    assert!(
        help.contains("Experimental: pass a moonrun TOML policy file"),
        "expected runwasm help to mark policy as experimental, got:\n{help}"
    );
    assert!(
        help.contains("WASI is not covered"),
        "expected runwasm help to explain policy scope, got:\n{help}"
    );
}

#[test]
fn test_runwasm_local_package_forwards_experimental_policy() {
    let dir = TestDir::new("moon_run_with_cli_args.in");
    let stderr = get_err_stderr_with_envs(
        &dir,
        [
            "runwasm",
            "--experimental-policy",
            "missing-policy.toml",
            "main",
        ],
        [("MOONRUN_OVERRIDE", moonrun_bin())],
    );
    assert!(
        stderr.contains("failed to load sandbox policy"),
        "expected sandbox policy load error, got:\n{stderr}"
    );
}

#[test]
fn test_run_with_explicit_target_exits_with_guest_exit_code() {
    let dir = TestDir::new("moon_run_with_cli_args.in");
    moon_cmd(&dir)
        .env("MOONRUN_OVERRIDE", moonrun_bin())
        .args(["run", "main", "--target", "wasm", "exit-7"])
        .assert()
        .code(7)
        .stdout_eq("");
}

#[test]
fn test_test_with_explicit_target_fails_on_test_executable_exit_code() {
    let dir = TestDir::new("moon_run_with_cli_args.in");
    let stderr = get_err_stderr_with_envs(
        &dir,
        ["test", "main", "--target", "wasm", "--filter", "exit-7"],
        [("MOONRUN_OVERRIDE", moonrun_bin())],
    );
    snapbox::assert_data_eq!(
        stderr,
        snapbox::str![[r#"
...
Error: failed to run test for target Wasm

Caused by:
    Failed to run the test: $ROOT/_build/wasm/debug/test/main/main.blackbox_test.wasm
    The test executable exited with exit [..]: 7
    Active test at executable exit:
      - $ROOT/main/exit_wasm_test.mbt:1 "exit-7"

"#]],
    );
}

#[test]
fn test_runwasm_exits_with_guest_exit_code() {
    let dir = TestDir::new("moon_run_with_cli_args.in");
    moon_cmd(&dir)
        .env("MOONRUN_OVERRIDE", moonrun_bin())
        .args(["runwasm", "main", "exit-7"])
        .assert()
        .code(7)
        .stdout_eq("");
}

#[test]
fn test_runwasm_cached_asset_exits_with_guest_exit_code() {
    let dir = TestDir::new("moon_run_with_cli_args.in");
    moon_cmd(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let moon_home = tempfile::TempDir::new().expect("failed to create temp MOON_HOME");
    let cache_path = moon_home
        .path()
        .join("registry/cache/assets/moonbitlang/parser/0.3.3/cmd/moonfmt/moonfmt.wasm");
    std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
    std::fs::copy(
        dir.join("_build/wasm/debug/build/main/main.wasm"),
        &cache_path,
    )
    .unwrap();

    moon_cmd(&dir)
        .env("MOON_HOME", moon_home.path())
        .env("MOONRUN_OVERRIDE", moonrun_bin())
        .args(["runwasm", "moonbitlang/parser/cmd/moonfmt@0.3.3", "exit-7"])
        .assert()
        .code(7)
        .stdout_eq("")
        .stderr_eq("");
}

#[test]
fn test_runwasm_cached_asset_forwards_experimental_policy() {
    let dir = TestDir::new("moon_run_with_cli_args.in");
    moon_cmd(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let moon_home = tempfile::TempDir::new().expect("failed to create temp MOON_HOME");
    let cache_path = moon_home
        .path()
        .join("registry/cache/assets/moonbitlang/parser/0.3.3/cmd/moonfmt/moonfmt.wasm");
    std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
    std::fs::copy(
        dir.join("_build/wasm/debug/build/main/main.wasm"),
        &cache_path,
    )
    .unwrap();

    let envs = vec![
        ("MOON_HOME".to_string(), moon_home.path().to_path_buf()),
        ("MOONRUN_OVERRIDE".to_string(), moonrun_bin()),
    ];
    let stderr = get_err_stderr_with_envs(
        &dir,
        [
            "runwasm",
            "--experimental-policy",
            "missing-policy.toml",
            "moonbitlang/parser/cmd/moonfmt@0.3.3",
        ],
        envs,
    );
    assert!(
        stderr.contains("failed to load sandbox policy"),
        "expected sandbox policy load error, got:\n{stderr}"
    );
}

#[test]
fn test_runwasm_uses_cached_asset_and_forwards_args() {
    let dir = TestDir::new("moon_run_with_cli_args.in");
    moon_cmd(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let moon_home = tempfile::TempDir::new().expect("failed to create temp MOON_HOME");
    let cache_path = moon_home
        .path()
        .join("registry/cache/assets/moonbitlang/parser/0.3.3/cmd/moonfmt/moonfmt.wasm");
    std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
    std::fs::copy(
        dir.join("_build/wasm/debug/build/main/main.wasm"),
        &cache_path,
    )
    .unwrap();

    moon_cmd(&dir)
        .env("MOON_HOME", moon_home.path())
        .args([
            "runwasm",
            "moonbitlang/parser/cmd/moonfmt@0.3.3",
            "--arg1",
            "arg2",
        ])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
[..]/registry/cache/assets/moonbitlang/parser/0.3.3/cmd/moonfmt/moonfmt.wasm
--arg1
arg2

"#]])
        .stderr_eq("");
}

#[test]
fn test_runwasm_rejects_existing_wasm_file() {
    let dir = TestDir::new_empty();
    std::fs::write(dir.join("main.wasm"), b"\0asmtest").unwrap();

    moon_cmd(&dir)
        .args(["runwasm", "main.wasm"])
        .assert()
        .failure()
        .stderr_eq("Error: `main.wasm` is not a package directory\n");
}

#[test]
fn test_runwasm_rejects_dry_run() {
    let dir = TestDir::new_empty();
    let stderr = get_err_stderr(
        &dir,
        [
            "--dry-run",
            "runwasm",
            "moonbitlang/parser/cmd/moonfmt@0.3.3",
        ],
    );
    assert!(
        stderr.contains("--dry-run is not supported for Mooncakes assets in `moon runwasm`"),
        "expected dry-run rejection, got:\n{stderr}"
    );
}

#[cfg(any(unix, windows))]
#[test]
fn test_external_subcommand_delegation_handles_interrupt() {
    let dir = TestDir::new_empty();
    let fake_bin_dir = tempfile::TempDir::new().expect("failed to create fake bin dir");
    let ready_file = fake_bin_dir.path().join("moon-test-behavior-ready");
    compile_signal_fixture(fake_bin_dir.path());

    let path = path_with_fake_bin(fake_bin_dir.path());
    let mut command = interruptible_moon_command(&dir);
    let mut child = command
        .arg("test-behavior")
        .arg(&ready_file)
        .env("PATH", path)
        .spawn()
        .expect("failed to spawn moon test-behavior");

    wait_for_ready_file(&mut child, &ready_file);
    send_interrupt(child.id());
    let status = wait_for_child_status(&mut child);
    assert_eq!(
        status.code(),
        Some(42),
        "fake moon-test-behavior did not handle interrupt itself; status was {status}"
    );
}

#[test]
fn test_cram_dry_run_builds_native_release_and_prints_delegation() {
    let dir = TestDir::new("hello");
    let stdout = get_stdout(
        &dir,
        ["--dry-run", "cram", "test", "--release", "--", "--list"],
    );

    assert!(
        stdout.contains("./_build/native/release/build/main/main.exe"),
        "dry-run should build the native release executable:\n{stdout}"
    );

    let cram_line = stdout.lines().last().unwrap_or_default();
    let path_separator = if cfg!(windows) { ";" } else { ":" };
    assert!(
        cram_line.contains(&format!(
            "'PATH=./_build/native/release/build/main{path_separator}$PATH'"
        )),
        "dry-run should print moon-cram with the computed PATH:\n{stdout}"
    );
    assert!(
        cram_line.contains("moon-cram") && cram_line.ends_with(" test --list"),
        "dry-run should print moon-cram with forwarded args:\n{stdout}"
    );
}

#[test]
fn test_cram_delegates_with_built_binary_dirs_on_path() {
    let dir = TestDir::new("hello");
    let fake_bin_dir = tempfile::TempDir::new().expect("failed to create fake bin dir");
    let fake_cram = compile_fake_cram_fixture(fake_bin_dir.path());

    let stdout = get_stdout_with_envs(
        &dir,
        ["cram", "test", "--shell", "bash"],
        [(
            "MOON_CRAM_OVERRIDE",
            fake_cram.to_string_lossy().into_owned(),
        )],
    );

    assert!(
        stdout.contains("fake-moon-cram-args=test|--shell|bash"),
        "moon-cram should receive passthrough args:\n{stdout}"
    );
    assert!(
        stdout.contains("fake-moon-cram-path=$ROOT/_build/native/debug/build/main"),
        "moon-cram PATH should include the built executable directory:\n{stdout}"
    );
}

#[test]
fn test_cram_makes_built_executable_available_on_path() {
    let dir = TestDir::new("cram");

    let stdout = get_stdout(&dir, ["cram", "test", "tests/cram"]);

    assert!(
        stdout.contains("Result: 1 document(s) with 1 testcase(s): 1 succeeded"),
        "moon-cram should run the cram test that invokes the built executable by name:\n{stdout}"
    );
}

#[test]
fn test_cram_test_help_shows_wrapper_options() {
    let dir = TestDir::new_empty();

    let stdout = get_stdout(&dir, ["cram", "test", "--help"]);

    assert!(
        stdout.contains("--release") && stdout.contains("--target <TARGET>"),
        "moon cram test help should show Moon-owned wrapper options:\n{stdout}"
    );
}

#[test]
fn test_cram_parent_help_shows_wrapper_subcommands() {
    let dir = TestDir::new_empty();

    let stdout = get_stdout(&dir, ["cram", "--help"]);

    assert!(
        stdout.contains("test") && stdout.contains("Build native executables"),
        "moon cram help should show Moon-owned wrapper subcommands:\n{stdout}"
    );
}

#[test]
fn test_cram_parent_flag_delegates_to_moon_cram() {
    let dir = TestDir::new_empty();
    let fake_bin_dir = tempfile::TempDir::new().expect("failed to create fake bin dir");
    let fake_cram = compile_fake_cram_fixture(fake_bin_dir.path());

    let stdout = get_stdout_with_envs(
        &dir,
        ["cram", "--version"],
        [(
            "MOON_CRAM_OVERRIDE",
            fake_cram.to_string_lossy().into_owned(),
        )],
    );

    assert!(
        stdout.contains("fake-moon-cram-args=--version"),
        "moon cram parent flags should be handled by moon-cram:\n{stdout}"
    );
}

#[test]
fn test_cram_parent_flag_after_moon_global_delegates_to_moon_cram() {
    let dir = TestDir::new_empty();
    let fake_bin_dir = tempfile::TempDir::new().expect("failed to create fake bin dir");
    let fake_cram = compile_fake_cram_fixture(fake_bin_dir.path());

    let stdout = get_stdout_with_envs(
        &dir,
        ["-q", "cram", "--version"],
        [(
            "MOON_CRAM_OVERRIDE",
            fake_cram.to_string_lossy().into_owned(),
        )],
    );

    assert!(
        stdout.contains("fake-moon-cram-args=--version"),
        "moon cram parent flags should still delegate after Moon globals:\n{stdout}"
    );
}

#[test]
fn test_cram_non_test_subcommand_delegates_to_moon_cram() {
    let dir = TestDir::new_empty();
    let fake_bin_dir = tempfile::TempDir::new().expect("failed to create fake bin dir");
    let fake_cram = compile_fake_cram_fixture(fake_bin_dir.path());

    let stdout = get_stdout_with_envs(
        &dir,
        ["cram", "list", "--json"],
        [(
            "MOON_CRAM_OVERRIDE",
            fake_cram.to_string_lossy().into_owned(),
        )],
    );

    assert!(
        stdout.contains("fake-moon-cram-args=list|--json"),
        "non-test cram invocations should be handled by moon-cram:\n{stdout}"
    );
}

#[test]
fn test_cram_build_path_error_does_not_delegate_to_moon_cram() {
    let dir = TestDir::new_empty();
    let fake_bin_dir = tempfile::TempDir::new().expect("failed to create fake bin dir");
    let fake_cram = compile_fake_cram_fixture(fake_bin_dir.path());

    let stderr = get_err_stderr_with_envs(
        &dir,
        ["build", "cram", "--bad-flag"],
        [(
            "MOON_CRAM_OVERRIDE",
            fake_cram.to_string_lossy().into_owned(),
        )],
    );

    assert!(
        stderr.contains("unexpected argument '--bad-flag'"),
        "cram path under moon build should stay a build parse error:\n{stderr}"
    );
}

#[test]
fn test_cram_rejects_unresolved_runner_before_build_path() {
    let dir = TestDir::new_empty();
    std::fs::write(
        dir.join("moon.mod.json"),
        r#"{ "name": "username/cram-shadow" }"#,
    )
    .expect("failed to write module manifest");
    std::fs::create_dir_all(dir.join("moon-cram")).expect("failed to create main package");
    std::fs::write(
        dir.join("moon-cram/moon.pkg.json"),
        r#"{ "is-main": true }"#,
    )
    .expect("failed to write package manifest");
    std::fs::write(
        dir.join("moon-cram/main.mbt"),
        "fn main { println(\"shadowed\") }\n",
    )
    .expect("failed to write main source");

    let empty_path = tempfile::TempDir::new().expect("failed to create empty PATH dir");
    let stderr = get_err_stderr_with_envs(
        &dir,
        ["cram", "test"],
        [
            ("MOON_CRAM_OVERRIDE".to_string(), "moon-cram".to_string()),
            (
                "PATH".to_string(),
                empty_path.path().to_string_lossy().into_owned(),
            ),
        ],
    );

    assert!(
        stderr.contains("no such subcommand: `cram`"),
        "unresolved moon-cram should fail before built executable dirs enter PATH:\n{stderr}"
    );
}

#[test]
fn test_cram_build_failure_prevents_delegation() {
    let dir = TestDir::new_empty();
    std::fs::write(dir.join("moon.mod.json"), r#"{ "name": "username/fail" }"#)
        .expect("failed to write module manifest");
    std::fs::create_dir_all(dir.join("main")).expect("failed to create main package");
    std::fs::write(dir.join("main/moon.pkg.json"), r#"{ "is-main": true }"#)
        .expect("failed to write package manifest");
    std::fs::write(dir.join("main/main.mbt"), "fn main { missing }\n")
        .expect("failed to write invalid source");

    let fake_bin_dir = tempfile::TempDir::new().expect("failed to create fake bin dir");
    let fake_cram = compile_fake_cram_fixture(fake_bin_dir.path());
    let marker = fake_bin_dir.path().join("moon-cram-ran");

    let _stderr = get_err_stderr_with_envs(
        &dir,
        ["cram", "test"],
        [
            (
                "MOON_CRAM_OVERRIDE",
                fake_cram.to_string_lossy().into_owned(),
            ),
            (
                "FAKE_MOON_CRAM_MARKER",
                marker.to_string_lossy().into_owned(),
            ),
        ],
    );

    assert!(
        !marker.exists(),
        "moon-cram should not run after build failure"
    );
}

#[cfg(unix)]
fn interruptible_moon_command(dir: &impl AsRef<std::path::Path>) -> std::process::Command {
    moon_process_cmd(dir)
}

#[cfg(windows)]
fn interruptible_moon_command(dir: &impl AsRef<std::path::Path>) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP;

    let mut command = moon_process_cmd(dir);
    command.creation_flags(CREATE_NEW_PROCESS_GROUP);
    command
}

#[cfg(unix)]
fn interruptible_command(program: &std::path::Path) -> std::process::Command {
    std::process::Command::new(program)
}

#[cfg(windows)]
fn interruptible_command(program: &std::path::Path) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP;

    let mut command = std::process::Command::new(program);
    command.creation_flags(CREATE_NEW_PROCESS_GROUP);
    command
}

#[cfg(unix)]
fn send_interrupt(pid: u32) {
    let rc = unsafe { libc::kill(pid as libc::pid_t, libc::SIGINT) };
    assert_eq!(
        rc,
        0,
        "failed to send SIGINT to delegated process: {}",
        std::io::Error::last_os_error()
    );
}

#[cfg(windows)]
fn send_interrupt(pid: u32) {
    use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, GenerateConsoleCtrlEvent};

    let ok = unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid) };
    assert_ne!(
        ok,
        0,
        "failed to send CTRL_BREAK_EVENT to delegated process group: {}",
        std::io::Error::last_os_error()
    );
}

#[cfg(any(unix, windows))]
fn compile_signal_fixture(fake_bin_dir: &std::path::Path) -> PathBuf {
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/external_signal_handler.rs");
    let executable = fake_bin_dir.join(format!(
        "moon-test-behavior{}",
        std::env::consts::EXE_SUFFIX
    ));
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = std::process::Command::new(rustc)
        .arg("--edition=2021")
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .status()
        .expect("failed to compile fake moon-test-behavior");
    assert!(
        status.success(),
        "failed to compile fake moon-test-behavior"
    );
    executable
}

fn compile_fake_cram_fixture(fake_bin_dir: &std::path::Path) -> PathBuf {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_moon_cram.rs");
    let executable = fake_bin_dir.join(format!("fake-moon-cram{}", std::env::consts::EXE_SUFFIX));
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = std::process::Command::new(rustc)
        .arg("--edition=2021")
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .status()
        .expect("failed to compile fake moon-cram");
    assert!(status.success(), "failed to compile fake moon-cram");
    executable
}

#[cfg(any(unix, windows))]
fn path_with_fake_bin(fake_bin_dir: &std::path::Path) -> std::ffi::OsString {
    std::env::join_paths(
        std::iter::once(fake_bin_dir.to_path_buf()).chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )),
    )
    .expect("failed to build PATH")
}

#[cfg(any(unix, windows))]
fn wait_for_ready_file(child: &mut std::process::Child, ready_file: &std::path::Path) {
    let start = std::time::Instant::now();
    loop {
        if ready_file.exists() {
            break;
        }
        if let Some(status) = child.try_wait().expect("failed to poll moon subprocess") {
            panic!("moon exited before fake moon-test-behavior was ready: {status}");
        }
        if start.elapsed() > std::time::Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("timed out waiting for fake moon-test-behavior to be ready");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[cfg(any(unix, windows))]
fn wait_for_child_status(child: &mut std::process::Child) -> std::process::ExitStatus {
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("failed to poll moon subprocess") {
            return status;
        }
        if start.elapsed() > std::time::Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("timed out waiting for fake moon-test-behavior to exit");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
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
fn test_moon_explain_diagnostic_lists_compiler_warnings_and_integrated_docs() {
    let dir = TestDir::new_empty();
    let output = get_stdout(&dir, ["explain", "--diagnostic"]);
    assert!(output.starts_with("Available warnings: \n"));
    assert!(output.contains("partial_match              Partial pattern matching.                                       11 error"));
    assert!(output.contains("note: default alert exceptions: alert_unsafe=off"));
    assert!(output.contains("Available non-warning diagnostics:"));
    assert!(output.contains("E4056  diagnostic method_duplicate"));
}

#[test]
fn test_moon_explain_diagnostics_number_uses_integrated_docs() {
    let dir = TestDir::new_empty();
    let output = get_stdout(&dir, ["explain", "--diagnostic", "2"]);
    assert!(output.starts_with("# E0002\n"));
    assert!(output.contains("Warning name: `unused_value`"));
    assert!(output.contains("Unused variable."));
}

#[test]
fn test_moon_explain_diagnostics_alias_uses_integrated_docs() {
    let dir = TestDir::new_empty();
    let output = get_stdout(&dir, ["explain", "--diagnostics", "2"]);
    assert!(output.starts_with("# E0002\n"));
    assert!(output.contains("Warning name: `unused_value`"));
}

#[test]
fn test_moon_explain_diagnostics_mnemonic_uses_integrated_docs() {
    let dir = TestDir::new_empty();
    let output = get_stdout(&dir, ["explain", "--diagnostic", "unused_value"]);
    assert!(output.contains("# E0001"));
    assert!(output.contains("# E0002"));
}

#[test]
fn test_moon_explain_diagnostics_name_uses_integrated_docs() {
    let dir = TestDir::new_empty();
    let output = get_stdout(&dir, ["explain", "--diagnostic", "method_duplicate"]);
    assert!(output.starts_with("# E4056\n"));
    assert!(output.contains("Compiler diagnostic name: `method_duplicate`"));
}

#[test]
fn test_moon_explain_attribute_lists_integrated_docs() {
    let dir = TestDir::new_empty();
    let output = get_stdout(&dir, ["explain", "--attribute"]);
    assert!(output.starts_with("Available attributes:\n"));
    assert!(output.contains("#alert"));
    assert!(output.contains("#coverage.skip"));
    assert!(output.contains("#visibility"));
}

#[test]
fn test_moon_explain_attribute_name_uses_integrated_docs() {
    let dir = TestDir::new_empty();
    let output = get_stdout(&dir, ["explain", "--attribute", "alert"]);
    assert!(output.starts_with("# Alert Attribute\n"));
    assert!(output.contains("The `#alert` attribute attaches a category and message to an API."));
}

#[test]
fn test_moon_explain_without_flags_shows_guidance() {
    let dir = TestDir::new_empty();
    check(
        get_err_stderr(&dir, ["explain"]),
        expect![[r#"
            Explain compiler diagnostics and language topics

            Usage: moon explain [OPTIONS] <--diagnostic [<ID_OR_NAME>]|--attribute [<NAME>]>

            Options:
                  --diagnostic [<ID_OR_NAME>]  Explain diagnostics. Without a query, list diagnostic codes and names
                  --attribute [<NAME>]         Explain attributes. Without a query, list attribute names
              -h, --help                       Print help

            Common Options:
                  --target-dir <TARGET_DIR>  The target directory. Defaults to `<project-root>/_build`
              -q, --quiet                    Suppress output
              -v, --verbose                  Increase verbosity
                  --trace                    Trace the execution of the program
                  --dry-run                  Do not actually run the command

            Resources:
                Docs: https://docs.moonbitlang.com
                Skills: https://github.com/moonbitlang/skills

                Use `moon explain --diagnostic` to list diagnostic codes and names.
                Use `moon explain --attribute` to list attributes.
        "#]],
    );
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
fn test_moon_run_help_displays_inline_script_as_e() {
    let dir = TestDir::new_empty();
    let stdout = get_stdout(&dir, ["help", "run"]).replace("moon.exe", "moon");
    assert!(stdout.contains("-e <SCRIPT>"), "stdout: {stdout}");
    assert!(!stdout.contains("-c <SCRIPT>"), "stdout: {stdout}");
}

#[test]
fn test_moon_run_command_string_reads_inline_script() {
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
        .stdout_eq("hello from run -e\n")
        .stderr_eq("");
}

#[test]
fn test_moon_run_command_string_short_alias_c_reads_inline_script() {
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
        .stdout_eq("hello from run -c\n")
        .stderr_eq("");
}

#[test]
fn test_moon_run_command_string_invalid_source_keeps_diagnostics_on_stderr() {
    let dir = TestDir::new_empty();
    let assert = moon_cmd(&dir)
        .args(["run", "-e", r#"println("hello")"#])
        .assert()
        .failure()
        .stdout_eq("");
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("Missing main function in the main package"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("Parse error, unexpected token"),
        "stderr: {stderr}"
    );
    assert!(!stderr.contains("failed:"), "stderr: {stderr}");
}

#[test]
fn test_moon_run_command_string_invalid_source_shows_failed_command_in_verbose_mode() {
    let dir = TestDir::new_empty();
    moon_cmd(&dir)
        .args(["run", "--verbose", "-e", r#"println("hello")"#])
        .assert()
        .failure()
        .stdout_eq("failed: [..]moonc[..] build-package [..]\n");
}

#[test]
fn test_moon_run_command_string_defaults_to_wasm() {
    let dir = TestDir::new_empty();
    let stdout = get_stdout(
        &dir,
        [
            "run",
            "-e",
            r#"fn main {
  println("hello from run -e")
}
"#,
            "--dry-run",
        ],
    );

    assert!(stdout.contains("-target wasm"), "stdout: {stdout}");
    assert!(
        stdout.contains("./_build/wasm/debug/build/single/single.core"),
        "stdout: {stdout}"
    );
    assert!(!stdout.contains("-target wasm-gc"), "stdout: {stdout}");
    assert!(!stdout.contains("-target native"), "stdout: {stdout}");
}

#[test]
fn test_moon_run_command_string_explicit_target_overrides_wasm_default() {
    let dir = TestDir::new_empty();
    let stdout = get_stdout(
        &dir,
        [
            "run",
            "-e",
            r#"fn main {
  println("hello from run -e")
}
"#,
            "--target",
            "wasm-gc",
            "--dry-run",
        ],
    );

    assert!(stdout.contains("-target wasm-gc"), "stdout: {stdout}");
    assert!(
        stdout.contains("moonrun ./_build/wasm-gc/debug/build/single/single.wasm --"),
        "stdout: {stdout}"
    );
    assert!(!stdout.contains("./_build/wasm/debug/"), "stdout: {stdout}");
    assert!(!stdout.contains("-target native"), "stdout: {stdout}");
}

#[test]
fn test_moon_run_command_string_short_alias_c_defaults_to_wasm() {
    let dir = TestDir::new_empty();
    let stdout = get_stdout(
        &dir,
        [
            "run",
            "-c",
            r#"fn main {
  println("hello from run -c")
}
"#,
            "--dry-run",
        ],
    );

    assert!(stdout.contains("-target wasm"), "stdout: {stdout}");
    assert!(
        stdout.contains("./_build/wasm/debug/build/single/single.core"),
        "stdout: {stdout}"
    );
    assert!(!stdout.contains("-target wasm-gc"), "stdout: {stdout}");
    assert!(!stdout.contains("-target native"), "stdout: {stdout}");
}

#[test]
fn test_moon_run_command_string_conflicts_with_other_inputs() {
    let dir = TestDir::new_empty();

    let dash_stderr = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("run")
        .arg("-e")
        .arg(r#"fn main { println("hello") }"#)
        .arg("-")
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();
    let dash_stderr = String::from_utf8_lossy(&dash_stderr);
    assert!(dash_stderr.contains("cannot be used with"));
    assert!(dash_stderr.contains("-e"));

    let path_stderr = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("run")
        .arg("-e")
        .arg(r#"fn main { println("hello") }"#)
        .arg("main")
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();
    let path_stderr = String::from_utf8_lossy(&path_stderr);
    assert!(path_stderr.contains("cannot be used with"));
    assert!(path_stderr.contains("-e"));
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
    assert!(out.contains("--upgrade"));
}

#[test]
fn test_manifest_path_is_not_supported() {
    let dir = TestDir::new("moon_commands");
    moon_cmd(&dir)
        .args(["check", "--manifest-path", "moon.mod.json", "--dry-run"])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
error: unexpected argument '--manifest-path' found

  tip: to pass '--manifest-path' as a value, use '-- --manifest-path'

Usage: moon[EXE] check [OPTIONS] [PATH]...

For more information, try '--help'.

"#]]);
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
