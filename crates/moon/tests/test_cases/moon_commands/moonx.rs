use crate::util::{moon_bin, moonrun_bin, toolchain_root_for_tests};
use crate::{TestDir, moon_cmd};

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
Run a standalone .mbtx file or a package from the Mooncakes registry without installing it.

Accepted input forms:
  moonx script.mbtx
  moonx user/module/package
  moonx user/module/package@1.2.3
  moonx user/module/package@latest

Standalone .mbtx files always use the linear-memory Wasm backend. Pinned
package coordinates use the requested version directly. `@latest` refreshes the
registry index before resolving the latest version. Unpinned coordinates use
the latest version already known to the local registry index.

Usage: moonx [OPTIONS] <MBTX|PACKAGE> [PROGRAM_ARGS]...

Options:
      --target <TARGET>
          [default: wasm]
          [possible values: wasm]

      --experimental-policy <PATH>
          Experimental moonrun policy file

      --daemon
          Share one execution host with descendants for this run (no persistent daemon)

  -v, --verbose
          Show progress and execution details

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version

"#]]);
}

#[test]
fn test_moonx_runs_mbtx_as_linear_wasm_and_forwards_args() {
    let dir = TestDir::new("moon_test_single_file.in");
    let bin_dir = tempfile::TempDir::new().expect("failed to create moonx bin directory");
    let moonx = moonx_bin(&bin_dir);

    snapbox::cmd::Command::new(&moonx)
        .current_dir(&dir)
        .env("MOON_TOOLCHAIN_ROOT", toolchain_root_for_tests())
        .env("MOONRUN_OVERRIDE", moonrun_bin())
        .args(["moonx_args.mbtx", "--help", "--target", "native", "-V"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
--help
--target
native
-V

"#]])
        .stderr_eq("");

    assert!(
        dir.join("_build/moonx_args.mbtx/wasm/debug/build/single/single.wasm")
            .is_file()
    );
    assert!(!dir.join("_build/moonx_args.mbtx/wasm-gc").exists());
}

#[test]
fn test_moonx_rejects_native_before_acquisition() {
    let dir = TestDir::new_empty();
    let bin_dir = tempfile::TempDir::new().unwrap();
    let moonx = moonx_bin(&bin_dir);
    for input in ["missing.mbtx", "user/module"] {
        snapbox::cmd::Command::new(&moonx)
            .current_dir(&dir)
            .env("MOON_HOME", dir.join("moon-home"))
            .args(["--target", "native", input])
            .assert()
            .code(2)
            .stdout_eq("")
            .stderr_eq(snapbox::str![[r#"
error: invalid value 'native' for '--target <TARGET>'
  [possible values: wasm]

For more information, try '--help'.

"#]]);
    }
    assert!(!dir.join("moon-home").exists());
    assert!(!dir.join("_build").exists());
}

#[test]
fn test_moonx_forwards_experimental_policy_to_mbtx() {
    let dir = TestDir::new("moon_test_single_file.in");
    let bin_dir = tempfile::TempDir::new().expect("failed to create moonx bin directory");
    let moonx = moonx_bin(&bin_dir);

    let output = std::process::Command::new(&moonx)
        .current_dir(&dir)
        .env("MOON_TOOLCHAIN_ROOT", toolchain_root_for_tests())
        .env("MOONRUN_OVERRIDE", moonrun_bin())
        .args([
            "--experimental-policy",
            "missing-policy.json",
            "moonx_args.mbtx",
        ])
        .output()
        .expect("failed to run moonx");
    let stderr = String::from_utf8(output.stderr).expect("moonx stderr should be UTF-8");

    assert!(!output.status.success());
    assert!(
        stderr.contains("failed to load sandbox policy"),
        "expected moonx to forward its policy to moonrun:\n{stderr}"
    );
}

#[test]
fn test_moonx_uses_embedded_mbtx_policy() {
    let dir = TestDir::new("moon_test_single_file.in");
    let bin_dir = tempfile::TempDir::new().expect("failed to create moonx bin directory");
    let moonx = moonx_bin(&bin_dir);

    snapbox::cmd::Command::new(&moonx)
        .current_dir(&dir)
        .env("MOON_TOOLCHAIN_ROOT", toolchain_root_for_tests())
        .env("MOONRUN_OVERRIDE", moonrun_bin())
        .arg("embedded_policy.mbtx")
        .assert()
        .success()
        .stdout_eq("embedded\n");
}

#[test]
fn test_moonx_explicit_policy_skips_malformed_embedded_policy() {
    let dir = TestDir::new("moon_test_single_file.in");
    let bin_dir = tempfile::TempDir::new().expect("failed to create moonx bin directory");
    let moonx = moonx_bin(&bin_dir);
    std::fs::write(
        dir.join("explicit-policy.json"),
        r#"{"env":{"set":{"MBTX_POLICY":"explicit"}}}"#,
    )
    .unwrap();

    snapbox::cmd::Command::new(&moonx)
        .current_dir(&dir)
        .env("MOON_TOOLCHAIN_ROOT", toolchain_root_for_tests())
        .env("MOONRUN_OVERRIDE", moonrun_bin())
        .args([
            "--experimental-policy",
            "explicit-policy.json",
            "malformed_embedded_policy.mbtx",
        ])
        .assert()
        .success()
        .stdout_eq("explicit\n");
}

#[test]
fn test_moonx_inherited_policy_rejects_malformed_child_policy() {
    let dir = TestDir::new("moon_test_single_file.in");
    let bin_dir = tempfile::TempDir::new().expect("failed to create moonx bin directory");
    let moonx = moonx_bin(&bin_dir);
    let inherited_policy = dir.join("inherited-policy.json");
    std::fs::write(
        &inherited_policy,
        include_bytes!("../../../../moonrun/tests/fixtures/deny_restrictions.json"),
    )
    .unwrap();
    let transfer = moonutil::policy_transport::PolicyTransfer::from_file(
        std::fs::File::open(inherited_policy).unwrap(),
    )
    .unwrap();
    let mut command = std::process::Command::new(&moonx);
    command
        .current_dir(&dir)
        .env("MOON_TOOLCHAIN_ROOT", toolchain_root_for_tests())
        .env("MOONRUN_OVERRIDE", moonrun_bin())
        .env("MBTX_POLICY", "inherited")
        .arg("malformed_embedded_policy.mbtx");
    let relay = transfer.into_relay().attach_to(&mut command).unwrap();

    snapbox::cmd::Command::from_std(command)
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
...
Error: failed to load sandbox policy (experimental); run `moonrun --help` for policy format notes

Caused by:
    0: invalid .mbtx permission in `[..]/malformed_embedded_policy.mbtx`
    1: policy.env: invalid type: sequence, expected struct EnvConfig at line 2 column 8

"#]]);
    relay.finish().unwrap();
}

#[test]
fn test_moonx_cached_wasm_preserves_args_exit_status_and_policy() {
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

    snapbox::cmd::Command::new(&moonx)
        .current_dir(&dir)
        .env("MOON_HOME", moon_home.path())
        .env("MOON_TOOLCHAIN_ROOT", toolchain_root_for_tests())
        .env("MOONRUN_OVERRIDE", moonrun_bin())
        .args([
            "--experimental-policy",
            "missing-policy.json",
            "moonbitlang/parser/cmd/moonfmt@0.3.3",
        ])
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
Error: failed to load sandbox policy (experimental); run `moonrun --help` for policy format notes

Caused by:
    0: failed to read policy missing-policy.json
    1: [..]

"#]]);
}
