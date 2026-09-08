use std::cell::OnceCell;

use crate::util::cache_registry_package;
use crate::{TestDir, assert_success, get_err_stderr, get_stdout_with_envs, moon_cmd};

// Notice the two `this-is-added-by-config-script`
#[test]
fn test_prebuild_config_js() {
    let dir = TestDir::new("prebuild_config_script/js");
    test_prebuild_config_common(dir);
}

#[test]
fn test_prebuild_config_py() {
    let dir = TestDir::new("prebuild_config_script/py");
    test_prebuild_config_common(dir);
}

#[test]
fn test_prebuild_config_mbtx() {
    let dir = TestDir::new("prebuild_config_script/mbtx");
    test_prebuild_config_common(dir);
}

#[test]
fn test_prebuild_config_mbtx_preserves_frozen_dependencies() {
    let dir = TestDir::new("prebuild_config_script/mbtx");
    let moon_home = tempfile::tempdir().unwrap();
    cache_registry_package(
        moon_home.path(),
        "testuser/prebuild-input",
        "0.1.0",
        &[
            (
                "moon.mod.json",
                br#"{"name":"testuser/prebuild-input","version":"0.1.0"}"#.to_vec(),
            ),
            ("moon.pkg.json", b"{}".to_vec()),
            (
                "lib.mbt",
                br#"pub fn value() -> String { "ready" }"#.to_vec(),
            ),
        ],
    );
    std::fs::write(
        dir.join("build config.mbtx"),
        r#"import { "testuser/prebuild-input@0.1.0" @input }
fn main {
  let output : Json = { "vars": { "HELLO": @input.value() } }
  println(output.stringify())
}
"#,
    )
    .unwrap();

    moon_cmd(&dir)
        .env("MOON_HOME", moon_home.path())
        .args(["build", "--target", "native", "--dry-run", "--frozen"])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
...
[..]`frozen` is set[..]
...
"#]]);

    moon_cmd(&dir)
        .env("MOON_HOME", moon_home.path())
        .env("MOON_OVERRIDE", dir.join("unused-moon"))
        .args(["build", "--target", "native", "--dry-run"])
        .assert()
        .success();
    moon_cmd(&dir)
        .env("MOON_HOME", moon_home.path())
        .env("MOON_OVERRIDE", dir.join("unused-moon"))
        .args(["build", "--target", "native", "--dry-run", "--frozen"])
        .assert()
        .success();
}

#[test]
fn test_prebuild_config_mbtx_keeps_dependency_sources_read_only() {
    let project = tempfile::tempdir().unwrap();
    let dependency = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("moon.mod.json"),
        serde_json::json!({
            "name": "testuser/consumer",
            "deps": { "testuser/config": { "path": dependency.path() } }
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        project.path().join("moon.pkg.json"),
        r#"{"import":["testuser/config"]}"#,
    )
    .unwrap();
    std::fs::write(
        project.path().join("lib.mbt"),
        "pub fn value() -> Int { @config.value() }",
    )
    .unwrap();
    std::fs::write(
        dependency.path().join("moon.mod.json"),
        r#"{"name":"testuser/config","version":"0.1.0","--moonbit-unstable-prebuild":"build.mbtx"}"#,
    )
    .unwrap();
    std::fs::write(dependency.path().join("moon.pkg.json"), "{}").unwrap();
    std::fs::write(
        dependency.path().join("lib.mbt"),
        "pub fn value() -> Int { 42 }",
    )
    .unwrap();
    std::fs::write(
        dependency.path().join("build.mbtx"),
        "fn main { println(\"{}\") }",
    )
    .unwrap();

    #[cfg(unix)]
    let permissions = {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::metadata(dependency.path()).unwrap().permissions();
        std::fs::set_permissions(dependency.path(), std::fs::Permissions::from_mode(0o555))
            .unwrap();
        permissions
    };
    let result = moon_cmd(&project)
        .arg("--target-dir")
        .arg(target.path())
        .args(["build", "--target", "native", "--dry-run"])
        .assert();
    #[cfg(unix)]
    std::fs::set_permissions(dependency.path(), permissions).unwrap();
    result.success();
    assert!(!dependency.path().join("_build").exists());
    assert!(!dependency.path().join(".mooncakes").exists());
    assert!(target.path().join("prebuild").is_dir());
}

#[test]
fn test_prebuild_config_mbtx_failure() {
    let dir = TestDir::new("prebuild_config_script/mbtx");
    std::fs::write(
        dir.join("build config.mbtx"),
        "fn main { abort(\"prebuild script failed\") }",
    )
    .unwrap();
    moon_cmd(&dir)
        .args(["build", "--target", "native", "--dry-run"])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
...
Error: failed to run build for target Native

Caused by:
    0: Failed to run prebuild script for module username/hello
    1: prebuild script `build config.mbtx` for module `username/hello@0.0.0 (local [..])` failed

"#]]);
}

#[test]
fn test_prebuild_config_mbtx_invalid_json() {
    let dir = TestDir::new("prebuild_config_script/mbtx");
    std::fs::write(
        dir.join("build config.mbtx"),
        "fn main { println(\"not json\") }",
    )
    .unwrap();
    moon_cmd(&dir)
        .args(["build", "--target", "native", "--dry-run"])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: failed to run build for target Native

Caused by:
    0: Failed to run prebuild script for module username/hello
    1: failed to deserialize prebuild script `build config.mbtx` for module `username/hello@0.0.0 (local [..])`
    2: expected ident at line 1 column 2

"#]]);
}

fn test_prebuild_config_common(dir: TestDir) {
    let cc = if cfg!(windows) { "cl" } else { "cc" };
    let stdout = get_stdout_with_envs(
        &dir,
        ["build", "--target", "native", "--dry-run"],
        [("MOON_CC", cc)],
    );
    println!("{}", &stdout);
    let lines = stdout.lines().collect::<Vec<_>>();

    let found_c_flags_replacement = OnceCell::<()>::new();
    let found_link_flags = OnceCell::<()>::new();
    for line in lines {
        if line.contains("stub.c") {
            found_c_flags_replacement
                .set(())
                .expect("c stub compilation found twice");
            assert!(line.contains("HELLO=------this-is-added-by-config-script------"));
        }

        if line.contains("cc -o ./_build/native/debug/build/main/main") && cfg!(unix) {
            found_link_flags.set(()).expect("final linking found twice");
            assert!(line.contains("-l______this_is_added_by_config_script_______"));
            assert!(line.contains("-lmylib"));
            assert!(line.contains("-L/my-search-path"));
        } else if (line.contains("cl.exe") // cl.exe might be quoted
            || line.starts_with("cl "))
            && line.contains("/Fe./_build/native/debug/build/main/main.exe")
            && cfg!(windows)
        {
            found_link_flags.set(()).expect("final linking found twice");
            assert!(line.contains("-l______this_is_added_by_config_script_______"));
            assert!(line.contains("mylib"));
            assert!(line.contains("/LIBPATH:/my-search-path"));
        }
    }
    found_c_flags_replacement
        .get()
        .expect("c stub compilation not found");
    found_link_flags.get().expect("link flags not found");
}

#[test]
fn test_prebuild_config_not_run_in_check() {
    let dir = TestDir::new("prebuild_config_script/check_skip_on_check");

    let build_err = get_err_stderr(&dir, ["build", "--target", "native", "--dry-run"]);
    assert!(
        build_err.contains("prebuild script `fail.js`"),
        "expected build to execute prebuild script and fail, got:\n{build_err}"
    );

    for target in ["native", "llvm"] {
        // Prebuild eligibility is decided during planning; LLVM standard-library
        // artifacts are not available on every platform running this test.
        assert_success(&dir, ["check", "--target", target, "--dry-run"]);
    }
}

#[test]
fn test_unstable_prebuild_config_only_runs_for_native_backends() {
    for target in ["wasm", "wasm-gc", "js"] {
        let dir = TestDir::new("prebuild_config_script/check_skip_on_check");
        moon_cmd(&dir)
            .args(["build", "--target", target, "--release"])
            .assert()
            .success();
        moon_cmd(&dir)
            .args(["build", "--target", target, "--release", "--dry-run"])
            .assert()
            .success();
    }

    for target in ["native", "llvm"] {
        let dir = TestDir::new("prebuild_config_script/check_skip_on_check");
        moon_cmd(&dir)
            .args(["build", "--target", target, "--release", "--dry-run"])
            .assert()
            .failure()
            .stderr_eq(snapbox::str![[r#"
...
[..]Failed to run prebuild script for module username/check_skip_on_check
...
"#]]);
    }
}

#[test]
fn test_unstable_prebuild_config_uses_preferred_backend() {
    for target in ["wasm", "wasm-gc", "js", "native", "llvm"] {
        let dir = TestDir::new("prebuild_config_script/check_skip_on_check");
        let manifest_path = dir.join("moon.mod.json");
        let mut manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
        manifest["preferred-target"] = target.into();
        std::fs::write(&manifest_path, serde_json::to_string(&manifest).unwrap()).unwrap();

        let result = moon_cmd(&dir)
            .args(["build", "--release", "--dry-run"])
            .assert();
        if matches!(target, "native" | "llvm") {
            result.failure().stderr_eq(snapbox::str![[r#"
...
[..]Failed to run prebuild script for module username/check_skip_on_check
...
"#]]);
        } else {
            result.success();
        }

        // An explicit backend overrides the module's preferred backend.
        moon_cmd(&dir)
            .args(["build", "--target", "wasm", "--release", "--dry-run"])
            .assert()
            .success();
    }
}

#[test]
fn test_unstable_prebuild_config_in_bin_dep_runs_for_check_install() {
    let top_dir = TestDir::new("prebuild_config_script/check_skip_bin_dep.in");
    let dir = top_dir.join("user.in");
    let generated_stub = top_dir.join("author.in/src/main/generated_stub.c");
    assert!(
        !generated_stub.exists(),
        "generated stub should not exist before check"
    );
    assert_success(&dir, ["check"]);
    assert!(
        generated_stub.exists(),
        "expected bin-dep prebuild to generate required stub during check install"
    );
}
