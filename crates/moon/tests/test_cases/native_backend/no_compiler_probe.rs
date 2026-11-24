use crate::{TestDir, moon_cmd};

#[test]
fn overridden_package_compilers_are_not_probed() {
    let dir = TestDir::new("native_backend/no_compiler_probe");
    let log = dir.join("unused-compiler.log");
    let compiler = dir.join("fake-toolchain/bin/probe-gcc");
    let compiler = compiler.to_str().unwrap();
    std::fs::write(
        dir.join("lib/moon.pkg"),
        format!(
            r#"options(
  "native-stub": ["stub.c"],
  link: {{ "native": {{ "cc": "{compiler}", "stub-cc": "{compiler}" }} }},
)
"#
        ),
    )
    .unwrap();
    std::fs::write(dir.join("lib/stub.c"), "int answer(void) { return 42; }\n").unwrap();

    for profile in ["--debug", "--release"] {
        moon_cmd(&dir)
            .env("MOON_CC", which::which("cc").unwrap())
            .env_remove("MOON_AR")
            .env_remove("MOON_CC_WRAPPER")
            .env("MOON_TEST_CC_PROBE_LOG", &log)
            .args(["test", "--target", "native", profile, "--dry-run"])
            .assert()
            .success();
    }
    assert!(
        !log.exists(),
        "package compiler was probed even though MOON_CC overrides it"
    );
}

#[test]
fn program_and_stub_compilers_share_one_probe() {
    let dir = TestDir::new("native_backend/no_compiler_probe");
    let log = dir.join("shared-compiler.log");
    let compiler = dir.join("fake-toolchain/bin/probe-gcc");
    let compiler = compiler.to_str().unwrap();
    // The test executable and its C stub request the same package compiler.
    std::fs::write(
        dir.join("lib/moon.pkg"),
        format!(
            r#"options(
  "native-stub": ["stub.c"],
  link: {{ "native": {{ "cc": "{compiler}", "stub-cc": "{compiler}" }} }},
)
"#
        ),
    )
    .unwrap();
    std::fs::write(dir.join("lib/stub.c"), "int answer(void) { return 42; }\n").unwrap();

    for profile in ["--debug", "--release"] {
        moon_cmd(&dir)
            .env_remove("MOON_CC")
            .env_remove("MOON_AR")
            .env_remove("MOON_CC_WRAPPER")
            .env("MOONBIT_NEW_NATIVE", "1")
            .env("MOON_TEST_CC_PROBE_LOG", &log)
            .args(["test", "--target", "native", profile, "--dry-run"])
            .assert()
            .success();
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "-dumpmachine\n");
        std::fs::remove_file(&log).unwrap();
    }
}

/// Run `moon` with a `MOON_CC` that logs each call, and return the calls.
fn compiler_calls(dir: &TestDir, args: &[&str]) -> String {
    let log = dir.join("probe.log");
    std::fs::write(&log, "").unwrap();
    moon_cmd(dir)
        .env("MOON_CC", "probe-gcc")
        .env_remove("MOON_AR")
        .env_remove("MOON_CC_WRAPPER")
        .env("MOONBIT_NEW_NATIVE", "1")
        .env("PATH", dir.join("fake-toolchain/bin"))
        .env("MOON_TEST_CC_PROBE_LOG", &log)
        .args(args)
        .assert()
        .success();
    std::fs::read_to_string(&log).unwrap()
}

#[test]
fn native_test_outline_does_not_probe_the_global_compiler() {
    let dir = TestDir::new("native_backend/no_compiler_probe");
    assert_eq!(
        compiler_calls(&dir, &["test", "--target", "native", "--outline"]),
        ""
    );
}

#[test]
fn native_source_actions_only_probe_the_compiler_target() {
    let dir = TestDir::new("native_backend/no_compiler_probe");
    let cc = dir.join("fake-toolchain/bin/probe-clang");
    std::fs::copy(dir.join("fake-toolchain/bin/probe-gcc"), &cc).unwrap();
    let log = dir.join("clang.log");
    for args in [
        &["check", "--target", "native"][..],
        &["build", "--target", "native", "--dry-run"],
    ] {
        std::fs::write(&log, "").unwrap();
        moon_cmd(&dir)
            .env("MOON_CC", &cc)
            .env_remove("MOON_AR")
            .env("MOON_TEST_CC_PROBE_LOG", &log)
            .args(args)
            .assert()
            .success();
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "-dumpmachine\n");
    }
}

#[test]
fn checking_native_sources_reports_an_unusable_global_compiler() {
    let dir = TestDir::new("native_backend/no_compiler_probe");
    moon_cmd(&dir)
        .env("MOON_CC", "missing-moon-test-gcc")
        .args(["check", "--target", "native"])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
...
[..]native compiler executable `missing-moon-test-gcc`[..]
...
"#]]);
}

#[test]
fn default_source_cfg_keeps_the_installed_core() {
    let dir = TestDir::new("native_backend/no_compiler_probe");
    let cc = dir.join("fake-toolchain/bin/probe-gcc");
    for (command, triple) in [
        // A Linux target and the Unix host share the default source cfg.
        ("build", "aarch64-unknown-linux-musl"),
        // Test discovery and test compilation both keep the host cfg.
        ("test", "x86_64-w64-mingw32"),
    ] {
        std::fs::write(&cc, format!("#!/bin/sh\necho {triple}\n")).unwrap();
        let output = crate::moon_process_cmd(&dir)
            .env("MOON_CC", &cc)
            .env_remove("MOON_AR")
            .args([command, "--release", "--target", "native", "--dry-run"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let plan = String::from_utf8(output.stdout).unwrap();
        assert!(plan.contains("/native/release/bundle/builtin/builtin.mi"));
        assert!(!plan.contains(" -cfg "));
    }
}

#[test]
fn non_native_test_builds_do_not_probe_compilers() {
    let dir = TestDir::new("native_backend/no_compiler_probe");
    for backend in ["js", "wasm", "wasm-gc", "wasm,wasm-gc,js"] {
        for profile in ["--debug", "--release"] {
            let calls = compiler_calls(&dir, &["test", "--target", backend, profile, "--dry-run"]);
            assert!(
                calls.is_empty(),
                "{backend} {profile} probed MOON_CC:\n{calls}"
            );
        }
    }
}

#[test]
fn native_test_builds_probe_the_environment_compiler_once() {
    let dir = TestDir::new("native_backend/no_compiler_probe");
    for targets in ["native", "wasm,wasm-gc,js,native"] {
        for profile in ["--debug", "--release"] {
            let calls = compiler_calls(&dir, &["test", "--target", targets, profile, "--dry-run"]);
            // Only the native plan needs the driver, shared by its runtime and program roles.
            assert_eq!(calls, "-dumpmachine\n", "{targets} {profile}");
        }
    }
}
