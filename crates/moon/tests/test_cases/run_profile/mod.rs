use super::*;

#[cfg(target_os = "macos")]
fn xcrun_version_shim() -> (tempfile::TempDir, String) {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().expect("failed to create temporary directory");
    let shim_path = tmp.path().join("xcrun");
    std::fs::write(
        &shim_path,
        "#!/usr/bin/env sh\nif [ \"$1\" = \"xctrace\" ] && [ \"$2\" = \"version\" ]; then\n  exit 0\nfi\nexit 0\n",
    )
    .unwrap();
    let mut perms = std::fs::metadata(&shim_path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&shim_path, perms).unwrap();

    let path = std::env::var("PATH")
        .map(|value| format!("{}:{}", tmp.path().display(), value))
        .unwrap_or_else(|_| tmp.path().display().to_string());
    (tmp, path)
}

#[cfg(target_os = "macos")]
fn profile_cwd_shim() -> (tempfile::TempDir, String) {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().expect("failed to create temporary directory");
    let shim_path = tmp.path().join("xcrun");
    std::fs::write(
        &shim_path,
        r#"#!/usr/bin/env sh
if [ "$1" = "xctrace" ] && [ "$2" = "version" ]; then
  exit 0
fi

if [ "$1" = "xctrace" ] && [ "$2" = "record" ]; then
  pwd > "$MOON_PROFILE_CWD_MARKER"
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "--" ]; then
      shift
      "$@" >/dev/null 2>/dev/null
      exit $?
    fi
    shift
  done
  exit 0
fi

if [ "$1" = "xctrace" ] && [ "$2" = "export" ]; then
  output=""
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "--output" ]; then
      shift
      output="$1"
    fi
    shift
  done
  cat > "$output" <<'XML'
<trace-query-result><node>
<row><thread-state id="1" fmt="Running">Running</thread-state><weight id="2" fmt="1.00 ms">1000000</weight><stack id="3" fmt="_M0profile"><frame id="4" name="_M0profile" addr="0x1"/></stack></row>
</node></trace-query-result>
XML
  exit 0
fi

exit 1
"#,
    )
    .unwrap();
    let mut perms = std::fs::metadata(&shim_path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&shim_path, perms).unwrap();

    let path = std::env::var("PATH")
        .map(|value| format!("{}:{}", tmp.path().display(), value))
        .unwrap_or_else(|_| tmp.path().display().to_string());
    (tmp, path)
}

#[cfg(target_os = "macos")]
#[test]
fn test_moon_run_profile_dry_run_prints_xctrace_commands() {
    use crate::dry_run_utils::line_with;

    let dir = TestDir::new("hello");
    let (_tmp, path) = xcrun_version_shim();

    let output = get_stdout_with_envs(
        &dir,
        ["run", "main", "--profile", "--dry-run"],
        [("PATH", path)],
    );

    let record_cmd = line_with(
        &output,
        "xcrun xctrace record",
        &[
            "--quiet",
            "--template",
            "Time",
            "Profiler",
            "--no-prompt",
            "--output",
        ],
    );
    let export_cmd = line_with(&output, "xcrun xctrace export", &["--quiet", "--input"]);

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
    let (_tmp, path) = xcrun_version_shim();
    let output = get_stdout_with_envs(&dir, ["test", "--profile", "--dry-run"], [("PATH", path)]);

    assert!(
        output.contains("_build/native/release/test"),
        "profile dry-run should use the native release test graph:\n{output}"
    );
    assert!(
        !output.contains("xcrun xctrace"),
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

#[cfg(target_os = "macos")]
#[test]
fn test_moon_test_profile_runs_profiler_from_module_root() {
    let dir = TestDir::new("run_profile/profile_test_cwd");
    let spawn_dir = dir.join("spawn");
    std::fs::create_dir(&spawn_dir).expect("failed to create spawn directory");
    let marker = dir.join("profile-cwd.txt");
    let (_tmp, path) = profile_cwd_shim();

    moon_cmd(&spawn_dir)
        .env("PATH", path)
        .env("MOON_PROFILE_CWD_MARKER", &marker)
        .args(["-C", "..", "test", "--profile"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
...
Samples: 1, sample weight: [..]ms
...
"#]]);

    let recorded = std::fs::read_to_string(&marker).expect("failed to read profiler cwd marker");
    let recorded = std::fs::canonicalize(recorded.trim()).expect("failed to canonicalize marker");
    let expected = std::fs::canonicalize(dir.as_ref()).expect("failed to canonicalize test dir");
    assert_eq!(recorded, expected);
}

#[cfg(target_os = "linux")]
#[test]
fn test_moon_run_profile_with_real_perf() {
    // Given a sustained MoonBit workload and real perf with DWARF unwinding
    // and permission to record per-process cpu-clock events.
    let dir = Fixture::readonly("run_profile/real_perf");

    // When the normal CLI records, exports, and parses the profile.
    moon_cmd(&dir)
        .env_remove("MOON_PERF_OVERRIDE")
        .args(["run", ".", "--profile"])
        .assert()
        .success();

    // Then the retained report must contain samples from the workload itself.
    let report = assert_perf_report(dir.target_dir().unwrap());
    assert_eq!(report["target"]["command"], "moon run --profile");
    for artifact in ["trace", "perf_script"] {
        let path = Path::new(report["artifacts"][artifact].as_str().unwrap());
        assert!(std::fs::metadata(path).unwrap().len() > 0);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn test_moon_test_profile_with_real_perf() {
    // Given the same real-perf workload, launched outside the module root.
    let dir = Fixture::copied("run_profile/real_perf");
    let spawn_dir = dir.join("spawn");
    std::fs::create_dir(&spawn_dir).unwrap();
    let tools = tempfile::tempdir().unwrap();
    let perf = tools.path().join("perf");
    std::os::unix::fs::symlink(which::which("perf").unwrap(), &perf).unwrap();

    // When Moon profiles the test executable and builds its aggregate report.
    moon_cmd(&dir)
        .current_dir(&spawn_dir)
        .env("MOON_PERF_OVERRIDE", &perf)
        .args(["-C", "..", "test", "--profile"])
        .assert()
        .success();

    // Then samples and artifacts must come from the real test workload.
    let report = assert_perf_report(&dir.join("_build"));
    assert_eq!(report["target"]["command"], "moon test --profile");
    let executables = report["executables"].as_array().unwrap();
    assert_eq!(executables.len(), 1);
    let artifacts = &executables[0]["artifacts"];
    for artifact in ["trace", "perf_script"] {
        let path = Path::new(artifacts[artifact].as_str().unwrap());
        assert!(std::fs::metadata(path).unwrap().len() > 0);
    }

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

#[cfg(target_os = "linux")]
fn assert_perf_report(target_dir: &Path) -> serde_json::Value {
    let reports: Vec<_> = walkdir::WalkDir::new(target_dir)
        .into_iter()
        .map(Result::unwrap)
        .filter(|entry| entry.file_name() == "profile.json")
        .map(|entry| entry.into_path())
        .collect();
    assert_eq!(reports.len(), 1, "expected one profile report");
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&reports[0]).unwrap()).unwrap();
    assert_eq!(report["producer"]["profiler_backend"], "perf");
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
