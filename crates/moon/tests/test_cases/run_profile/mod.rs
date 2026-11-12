use super::*;

// Concurrent xctrace recordings contend for the kernel tracing session.
#[cfg(target_os = "macos")]
static XCTRACE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(target_os = "macos")]
#[test]
fn test_moon_run_profile_dry_run_uses_xcrun_override() {
    use crate::dry_run_utils::line_with;

    let dir = TestDir::new("hello");
    let tools = tempfile::tempdir().unwrap();
    let xcrun = tools.path().join("xcrun");
    std::os::unix::fs::symlink(
        moonutil::toolchain::BINARIES.xcrun.as_ref().unwrap(),
        &xcrun,
    )
    .unwrap();

    let output = get_stdout_with_envs(
        &dir,
        ["run", "main", "--profile", "--dry-run"],
        [("MOON_XCRUN_OVERRIDE", &xcrun)],
    );

    let record_cmd = line_with(
        &output,
        "xctrace record",
        &[
            "--quiet",
            "--template",
            "Time",
            "Profiler",
            "--no-prompt",
            "--output",
        ],
    );
    let export_cmd = line_with(&output, "xctrace export", &["--quiet", "--input"]);
    for command in [&record_cmd, &export_cmd] {
        assert!(
            command.contains(xcrun.to_str().unwrap()),
            "command should use MOON_XCRUN_OVERRIDE: {command}"
        );
    }

    assert!(
        record_cmd.contains("--target-stdout"),
        "record command missing --target-stdout: {record_cmd}"
    );
    assert!(
        record_cmd.contains("--launch"),
        "record command missing --launch: {record_cmd}"
    );
    assert!(
        record_cmd.contains("_build/native/release/profile/main"),
        "record command missing profile output path: {record_cmd}"
    );
    assert!(
        export_cmd.contains("time-profile.xml"),
        "export command missing time-profile.xml output: {export_cmd}"
    );
    assert!(
        !output.lines().any(|line| line
            .trim_start()
            .starts_with("./_build/native/release/build/main/main.exe")),
        "profile dry-run should not print the standalone executable invocation:\n{output}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_moon_test_profile_dry_run_uses_native_release_graph() {
    let dir = TestDir::new("hello");
    let output = get_stdout(&dir, ["test", "--profile", "--dry-run"]);

    assert!(
        output.contains("_build/native/release/test"),
        "profile dry-run should use the native release test graph:\n{output}"
    );
    assert!(
        !output.contains("xctrace record"),
        "test profile dry-run should not print xctrace commands without generated test metadata:\n{output}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn test_moon_run_profile_dry_run_uses_perf_override() {
    use crate::dry_run_utils::line_with;

    let dir = TestDir::new("hello");
    let tools = tempfile::tempdir().unwrap();
    let perf = tools.path().join("perf");
    std::os::unix::fs::symlink(which::which("perf").unwrap(), &perf).unwrap();
    let output = get_stdout_with_envs(
        &dir,
        ["run", "main", "--profile", "--dry-run"],
        [("MOON_PERF_OVERRIDE", &perf)],
    );

    let record_cmd = line_with(
        &output,
        "perf record",
        &[
            "--quiet",
            "-F",
            "100",
            "-e",
            "cpu-clock",
            "--sample-cpu",
            "--call-graph",
            "dwarf",
            "-o",
        ],
    );
    let script_cmd = line_with(&output, "perf script", &["-i"]);
    for command in [&record_cmd, &script_cmd] {
        assert!(
            command.contains(perf.to_str().unwrap()),
            "command should use MOON_PERF_OVERRIDE: {command}"
        );
    }

    assert!(
        record_cmd.contains("perf.data"),
        "record command missing perf data output path: {record_cmd}"
    );
    assert!(
        record_cmd.contains("stdout.txt"),
        "record command missing stdout redirection: {record_cmd}"
    );
    assert!(
        record_cmd.contains("stderr.txt"),
        "record command missing stderr redirection: {record_cmd}"
    );
    assert!(
        script_cmd.contains("perf-script.txt"),
        "script command missing exported script output: {script_cmd}"
    );
    assert!(
        !output.lines().any(|line| line
            .trim_start()
            .starts_with("./_build/native/release/build/main/main.exe")),
        "profile dry-run should not print the standalone executable invocation:\n{output}"
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn test_moon_run_profile_with_real_profiler() {
    #[cfg(target_os = "macos")]
    let _recording = XCTRACE_LOCK.lock().unwrap();

    // Given a sustained MoonBit workload and the platform profiler, with
    // permission to record and unwind the native executable.
    let dir = Fixture::readonly("run_profile/workload");

    // When the normal CLI records, exports, and parses the profile.
    moon_cmd(&dir)
        .env_remove("MOON_PERF_OVERRIDE")
        .env_remove("MOON_XCRUN_OVERRIDE")
        .args(["run", ".", "--profile"])
        .assert()
        .success();

    // Then the retained report must contain samples from the workload itself.
    let report = assert_profile_report(dir.target_dir().unwrap());
    assert_eq!(report["target"]["command"], "moon run --profile");
    assert_profile_artifacts(&report["artifacts"]);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn test_moon_test_profile_with_real_profiler() {
    #[cfg(target_os = "macos")]
    let _recording = XCTRACE_LOCK.lock().unwrap();

    // Given the same workload, launched outside the module root, and an
    // explicit override pointing to the real platform profiler.
    let dir = Fixture::copied("run_profile/workload");
    let spawn_dir = dir.join("spawn");
    std::fs::create_dir(&spawn_dir).unwrap();
    let tools = tempfile::tempdir().unwrap();
    #[cfg(target_os = "linux")]
    let (profiler, override_var) = (
        moonutil::toolchain::BINARIES.perf.as_ref().unwrap(),
        "MOON_PERF_OVERRIDE",
    );
    #[cfg(target_os = "macos")]
    let (profiler, override_var) = (
        moonutil::toolchain::BINARIES.xcrun.as_ref().unwrap(),
        "MOON_XCRUN_OVERRIDE",
    );
    let selected_profiler = tools.path().join(profiler.file_name().unwrap());
    std::os::unix::fs::symlink(profiler, &selected_profiler).unwrap();

    // When Moon profiles the test executable and builds its aggregate report.
    moon_cmd(&dir)
        .current_dir(&spawn_dir)
        .env(override_var, &selected_profiler)
        .args(["-C", "..", "test", "--profile"])
        .assert()
        .success();

    // Then samples and artifacts must come from the real test workload.
    let report = assert_profile_report(&dir.join("_build"));
    assert_eq!(report["target"]["command"], "moon test --profile");
    let executables = report["executables"].as_array().unwrap();
    assert_eq!(executables.len(), 1);
    let artifacts = &executables[0]["artifacts"];
    assert_profile_artifacts(artifacts);

    // The test process itself reports its cwd, so this also checks that the
    // real profiler launches it from the module root.
    let stdout = std::fs::read_to_string(artifacts["stdout"].as_str().unwrap()).unwrap();
    let recorded = stdout
        .lines()
        .find_map(|line| line.strip_prefix("cwd: "))
        .expect("test workload should report its working directory");
    assert_eq!(
        std::fs::canonicalize(recorded).unwrap(),
        std::fs::canonicalize(dir.cwd()).unwrap()
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn assert_profile_report(target_dir: &Path) -> serde_json::Value {
    let reports: Vec<_> = walkdir::WalkDir::new(target_dir)
        .into_iter()
        .map(Result::unwrap)
        .filter(|entry| entry.file_name() == "profile.json")
        .map(|entry| entry.into_path())
        .collect();
    assert_eq!(reports.len(), 1, "expected one profile report");
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&reports[0]).unwrap()).unwrap();
    assert_eq!(
        report["producer"]["profiler_backend"],
        if cfg!(target_os = "macos") {
            "xctrace"
        } else {
            "perf"
        }
    );
    assert!(
        report["summary"]["profiled_samples"].as_u64().unwrap() > 0,
        "expected nonzero MoonBit samples: {report:#}"
    );
    assert!(
        report["rankings"]["inclusive_time"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| {
                entry["symbol"].as_str().unwrap().contains("fib")
                    && entry["samples"].as_u64().unwrap() > 0
            }),
        "expected samples attributed to fib: {report:#}"
    );
    report
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn assert_profile_artifacts(artifacts: &serde_json::Value) {
    let trace = Path::new(artifacts["trace"].as_str().unwrap());
    #[cfg(target_os = "macos")]
    assert!(trace.is_dir());
    #[cfg(target_os = "linux")]
    assert!(std::fs::metadata(trace).unwrap().len() > 0);
    let export = if cfg!(target_os = "macos") {
        "time_profile_xml"
    } else {
        "perf_script"
    };
    let path = Path::new(artifacts[export].as_str().unwrap());
    assert!(std::fs::metadata(path).unwrap().len() > 0);
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
#[test]
fn test_moon_run_profile_on_non_macos_reports_error() {
    use crate::get_err_stderr;
    let dir = TestDir::new("hello");
    let output = get_err_stderr(&dir, ["run", "main", "--profile"]);

    assert!(
        output.contains("`moon run --profile` currently supports macOS and Linux only"),
        "unexpected unsupported-platform error: {output}"
    );
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
#[test]
fn test_moon_test_profile_on_non_macos_reports_error() {
    use crate::get_err_stderr;
    let dir = TestDir::new("hello");
    let output = get_err_stderr(&dir, ["test", "--profile"]);

    assert!(
        output.contains("`moon test --profile` currently supports macOS and Linux only"),
        "unexpected unsupported-platform error: {output}"
    );
}
