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
fn native_source_commands_probe_the_global_compiler_for_its_platform() {
    let dir = TestDir::new("native_backend/no_compiler_probe");

    for args in [
        &["check", "--target", "native"][..],
        &["build", "--target", "native", "--dry-run"],
        &["test", "--target", "native", "--outline"],
    ] {
        let calls = compiler_calls(&dir, args);
        assert_eq!(calls, "-dumpmachine\n", "moon {}", args.join(" "));
    }
}

#[test]
fn non_windows_cross_target_keeps_default_cfg_and_installed_core() {
    let dir = TestDir::new("native_backend/no_compiler_probe");
    let cc = dir.join("fake-toolchain/bin/probe-gcc");
    // Both Unix hosts and this Linux target have the same default Windows predicate.
    std::fs::write(&cc, "#!/bin/sh\necho aarch64-unknown-linux-musl\n").unwrap();
    let output = crate::moon_process_cmd(&dir)
        .env("MOON_CC", cc)
        .args(["build", "--release", "--target", "native", "--dry-run"])
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

#[test]
fn non_native_test_builds_do_not_probe_compilers() {
    let dir = TestDir::new("native_backend/no_compiler_probe");
    for backend in ["js", "wasm", "wasm-gc"] {
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
    for profile in ["--debug", "--release"] {
        let calls = compiler_calls(&dir, &["test", "--target", "native", profile, "--dry-run"]);
        // Runtime and program roles, including payload selection, share the probe.
        assert_eq!(calls, "-dumpmachine\n", "{profile}");
    }
}
