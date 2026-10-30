use crate::{TestDir, get_stdout_with_envs};

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
            ("PROBE_LOG", log.display().to_string()),
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
    assert!(
        calls.lines().any(|call| call == "-dumpmachine"),
        "`moon test` did not probe MOON_CC:\n{calls}"
    );
}
