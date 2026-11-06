use crate::util::moonrun_bin;
use crate::{TestDir, get_err_stderr, get_err_stderr_with_envs, get_stdout, moon_cmd};

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
        help.contains("Local packages: use `moon run <LOCAL_PACKAGE> --target wasm`."),
        "expected runwasm help to identify the local-package replacement, got:\n{help}"
    );
    assert!(
        help.contains("Registry packages: use `moonx <PACKAGE[@VERSION]>`."),
        "expected runwasm help to identify the registry-package replacement, got:\n{help}"
    );
    assert!(
        help.contains("--experimental-policy <PATH>"),
        "expected runwasm help to expose experimental policy flag, got:\n{help}"
    );
    assert!(
        help.contains("Experimental: pass a moonrun JSON policy file"),
        "expected runwasm help to mark policy as experimental, got:\n{help}"
    );
    assert!(
        !help.contains("TOML"),
        "expected runwasm help to advertise only JSON policies, got:\n{help}"
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
        stderr.contains(
            "`moon runwasm` is deprecated and scheduled for removal after 2026-09-14; use `moon run <PACKAGE> --target wasm` instead."
        ),
        "expected local runwasm deprecation guidance, got:\n{stderr}"
    );
    assert!(
        stderr.contains("failed to load sandbox policy"),
        "expected sandbox policy load error, got:\n{stderr}"
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
        .stderr_eq(
            "Warning: `moon runwasm` is deprecated and scheduled for removal after 2026-09-14; use `moonx` instead.\n",
        );
}

#[test]
fn test_runwasm_cached_asset_flushes_trace_before_delegation() {
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
        .args([
            "--trace",
            "runwasm",
            "moonbitlang/parser/cmd/moonfmt@0.3.3",
            "exit-7",
        ])
        .assert()
        .code(7);

    let trace = std::fs::read_to_string(dir.join("trace.json")).unwrap();
    let events: serde_json::Value = serde_json::from_str(&trace).unwrap();
    assert!(events.as_array().is_some_and(|events| !events.is_empty()));
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
        stderr.contains(
            "`moon runwasm` is deprecated and scheduled for removal after 2026-09-14; use `moonx` instead."
        ),
        "expected registry runwasm deprecation guidance, got:\n{stderr}"
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
        .stderr_eq(
            "Warning: `moon runwasm` is deprecated and scheduled for removal after 2026-09-14; use `moonx` instead.\n",
        );
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
