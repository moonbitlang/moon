use crate::{TestDir, get_stdout_with_envs, moon_cmd};

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
fn repeated_package_compiler_requests_share_one_probe() {
    let dir = TestDir::new("native_backend/no_compiler_probe");
    let log = dir.join("shared-compiler.log");
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
    moon_cmd(&dir)
        .env_remove("MOON_CC")
        .env_remove("MOON_AR")
        .env("MOON_TEST_CC_PROBE_LOG", &log)
        .args(["test", "--target", "native", "--release", "--dry-run"])
        .assert()
        .success();
    let calls = std::fs::read_to_string(log).unwrap();
    assert_eq!(
        calls.lines().filter(|line| *line == "-dumpmachine").count(),
        1
    );
}

/// Run `moon` with a `MOON_CC` that logs each call, and return the calls.
fn compiler_calls(dir: &TestDir, args: &[&str]) -> String {
    let log = dir.join("probe.log");
    let _ = std::fs::remove_file(&log);
    get_stdout_with_envs(
        dir,
        args,
        [
            ("MOON_CC", "probe-gcc".to_string()),
            ("PATH", dir.join("fake-toolchain/bin").display().to_string()),
            ("MOON_TEST_CC_PROBE_LOG", log.display().to_string()),
        ],
    );
    std::fs::read_to_string(&log).unwrap_or_default()
}

#[test]
fn test_commands_without_native_executables_do_not_probe_moon_cc() {
    let dir = TestDir::new("native_backend/no_compiler_probe");

    for args in [
        &["check", "--target", "native"][..],
        &["build", "--target", "native", "--dry-run"],
        &["test", "--target", "native", "--outline"],
        &["test", "--target", "js", "--debug", "--dry-run"],
        &["test", "--target", "js", "--release", "--dry-run"],
        &["test", "--target", "wasm", "--debug", "--dry-run"],
        &["test", "--target", "wasm", "--release", "--dry-run"],
        &["test", "--target", "wasm-gc", "--debug", "--dry-run"],
        &["test", "--target", "wasm-gc", "--release", "--dry-run"],
    ] {
        let calls = compiler_calls(&dir, args);
        assert!(
            calls.is_empty(),
            "`moon {}` probed MOON_CC:\n{calls}",
            args.join(" ")
        );
    }

    // A debug test build requests native executables, so it checks whether
    // MOON_CC targets the host.
    let calls = compiler_calls(&dir, &["test", "--target", "native", "--dry-run"]);
    assert_eq!(
        calls.lines().filter(|call| *call == "-dumpmachine").count(),
        1,
        "payload selection and build planning must share the probe: {calls}"
    );
}
