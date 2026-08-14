use expect_test::expect;

use crate::{
    TestDir, get_stderr, get_stdout, moon_bin, moon_cmd,
    util::{cache_registry_package, check},
};

fn parse_complete_json(assert: snapbox::cmd::OutputAssert) -> serde_json::Value {
    let assert = assert.stderr_eq("");
    let output = assert.get_output();
    serde_json::from_slice(&output.stdout).expect("stdout should contain one complete JSON value")
}

fn check_json_with_postadd(postadd: &str) -> snapbox::cmd::OutputAssert {
    let dir = TestDir::new_empty();
    let moon_home = tempfile::TempDir::new().expect("failed to create temporary MOON_HOME");
    let moon_path = std::env::join_paths(
        std::iter::once(
            moon_bin()
                .parent()
                .expect("test moon binary should have a parent directory")
                .to_path_buf(),
        )
        .chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )),
    )
    .expect("test PATH should be valid");
    std::fs::create_dir_all(dir.join("src/lib")).unwrap();
    std::fs::write(
        dir.join("moon.mod.json"),
        r#"{
            "name": "test/root",
            "version": "0.1.0",
            "source": "src",
            "deps": { "testuser/postadd": "1.0.0" }
        }"#,
    )
    .unwrap();
    std::fs::write(dir.join("src/lib/moon.pkg.json"), "{}").unwrap();
    std::fs::write(dir.join("src/lib/lib.mbt"), "pub fn answer() -> Int { 42 }").unwrap();

    let dependency_manifest = serde_json::json!({
        "name": "testuser/postadd",
        "version": "1.0.0",
        "scripts": { "postadd": postadd }
    })
    .to_string()
    .into_bytes();
    cache_registry_package(
        moon_home.path(),
        "testuser/postadd",
        "1.0.0",
        &[("moon.mod.json", dependency_manifest)],
    );

    moon_cmd(&dir)
        .env("MOON_HOME", moon_home.path())
        .env("PATH", moon_path)
        .args(["check", "--json"])
        .assert()
}

#[test]
fn test_moon_check_complete_json_success() {
    let dir = TestDir::new("warns/deny_warn");
    let report = parse_complete_json(
        moon_cmd(&dir)
            .args(["check", "--json", "--sort-input", "-j1"])
            .assert()
            .success(),
    );

    assert_eq!(report["version"], 1);
    assert_eq!(report["status"], "success");
    assert_eq!(report["messages"], serde_json::json!([]));
    assert_eq!(report["diagnostics"].as_array().unwrap().len(), 4);
    assert_eq!(report["summary"]["diagnostic_errors"], 0);
    assert_eq!(report["summary"]["diagnostic_warnings"], 4);
    assert!(
        report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .all(|diagnostic| diagnostic["$message_type"] == "diagnostic")
    );
}

#[test]
fn test_moon_check_complete_json_flushes_trace_on_success() {
    let dir = TestDir::new("warns/deny_warn");
    let report = parse_complete_json(
        moon_cmd(&dir)
            .args(["--trace", "check", "--json", "--sort-input", "-j1"])
            .assert()
            .success(),
    );

    assert_eq!(report["status"], "success");
    let trace = std::fs::read_to_string(dir.join("trace.json")).unwrap();
    let events: serde_json::Value = serde_json::from_str(&trace).unwrap();
    assert!(events.as_array().is_some_and(|events| !events.is_empty()));
}

#[test]
fn test_moon_check_complete_json_compiler_failure() {
    let dir = TestDir::new("dedup_diag_error_limit.in");
    let report = parse_complete_json(
        moon_cmd(&dir)
            .args(["check", "--json", "--diagnostic-limit", "1", "-j1"])
            .assert()
            .failure(),
    );

    assert_eq!(report["status"], "failure");
    assert_eq!(report["diagnostics"].as_array().unwrap().len(), 1);
    assert_eq!(report["diagnostics"][0]["level"], "error");
    assert_eq!(report["diagnostics"][0]["target_backend"], "wasm");
    assert_eq!(report["messages"][0]["$message_type"], "moon");
    assert_eq!(report["messages"][0]["level"], "warning");
    assert_eq!(report["summary"]["moon_warnings"], 1);
    assert_eq!(report["summary"]["diagnostic_errors"], 1);
    assert_eq!(report["summary"]["diagnostic_warnings"], 3);
}

#[test]
fn test_moon_check_complete_json_flushes_trace_on_failure() {
    let dir = TestDir::new("dedup_diag_error_limit.in");
    let report = parse_complete_json(
        moon_cmd(&dir)
            .args([
                "--trace",
                "check",
                "--json",
                "--diagnostic-limit",
                "1",
                "-j1",
            ])
            .assert()
            .failure(),
    );

    assert_eq!(report["status"], "failure");
    let trace = std::fs::read_to_string(dir.join("trace.json")).unwrap();
    let events: serde_json::Value = serde_json::from_str(&trace).unwrap();
    assert!(events.as_array().is_some_and(|events| !events.is_empty()));
}

#[test]
fn test_moon_check_complete_json_collects_every_planned_backend() {
    let dir = TestDir::new("workspace_conflicting_preferred_targets.in");
    std::fs::write(
        dir.join("js_preferred/src/lib/lib.mbt"),
        "pub fn broken_js() -> Int { \"js\" }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("native_preferred/src/lib/lib.mbt"),
        "pub fn broken_native() -> Int { \"native\" }\n",
    )
    .unwrap();

    let report = parse_complete_json(
        moon_cmd(&dir)
            .args(["check", "--json", "--sort-input", "-j1"])
            .assert()
            .failure(),
    );
    let diagnostics = report["diagnostics"].as_array().unwrap();

    assert_eq!(report["status"], "failure");
    assert_eq!(report["summary"]["tasks_executed"], serde_json::Value::Null);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic["target_backend"] == "js"
            && diagnostic["path"]
                .as_str()
                .unwrap()
                .contains("js_preferred")
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic["target_backend"] == "native"
            && diagnostic["path"]
                .as_str()
                .unwrap()
                .contains("native_preferred")
    }));
}

#[test]
fn test_moon_check_complete_json_target_all_preserves_backend_provenance() {
    let dir = TestDir::new("dedup_diag_error_limit.in");

    moon_cmd(&dir)
        .args([
            "check",
            "--json",
            "--target",
            "all",
            "--diagnostic-limit",
            "1",
            "--sort-input",
            "-j1",
        ])
        .assert()
        .failure()
        .stderr_eq("")
        .stdout_eq(snapbox::str![[r#"
{"version":1,"status":"failure","diagnostics":[[..]"target_backend":"wasm"[..]"target_backend":"wasm-gc"[..]"target_backend":"js"[..]"target_backend":"native"[..]],"messages":[{"$message_type":"moon","level":"warning","message":"diagnostic output limited by --diagnostic-limit: 0 errors and 12 warnings were not displayed."}],"summary":{"tasks_executed":null,"moon_errors":0,"moon_warnings":1,"diagnostic_errors":4,"diagnostic_warnings":12,"hidden_diagnostic_warnings":12}}

"#]]);
}

#[test]
fn test_moon_check_complete_json_quiet_filters_moon_warnings() {
    let dir = TestDir::new("dedup_diag_error_limit.in");
    let report = parse_complete_json(
        moon_cmd(&dir)
            .args([
                "check",
                "--json",
                "--diagnostic-limit",
                "1",
                "--quiet",
                "-j1",
            ])
            .assert()
            .failure(),
    );

    assert_eq!(report["messages"], serde_json::json!([]));
    assert_eq!(report["summary"]["moon_warnings"], 0);
    assert_eq!(report["summary"]["diagnostic_errors"], 1);
    assert_eq!(report["summary"]["diagnostic_warnings"], 3);
}

#[test]
fn test_moon_check_complete_json_project_not_found() {
    let dir = TestDir::new_empty();
    let report = parse_complete_json(moon_cmd(&dir).args(["check", "--json"]).assert().failure());

    assert_eq!(report["status"], "failure");
    assert_eq!(report["diagnostics"], serde_json::json!([]));
    assert_eq!(report["messages"][0]["$message_type"], "moon");
    assert_eq!(report["messages"][0]["level"], "error");
    assert!(
        report["messages"][0]["message"]
            .as_str()
            .unwrap()
            .contains("not in a Moon project")
    );
    assert_eq!(report["summary"]["tasks_executed"], serde_json::Value::Null);
    assert_eq!(report["summary"]["moon_errors"], 1);
}

#[test]
fn test_moon_check_complete_json_captures_moon_warnings() {
    let dir = TestDir::new("fmt_moon_work_existing.in");
    let report = parse_complete_json(moon_cmd(&dir).args(["check", "--json"]).assert().success());

    assert_eq!(report["status"], "success");
    assert_eq!(report["messages"][0]["$message_type"], "moon");
    assert_eq!(report["messages"][0]["level"], "warning");
    assert!(
        report["messages"][0]["message"]
            .as_str()
            .unwrap()
            .contains("preferred_target")
    );
    assert_eq!(report["summary"]["moon_warnings"], 1);
}

#[test]
fn test_moon_check_complete_json_captures_manifest_warnings() {
    let dir = TestDir::new("fmt_moon_mod_both.in");
    let report = parse_complete_json(moon_cmd(&dir).args(["check", "--json"]).assert().success());

    assert_eq!(report["messages"][0]["level"], "warning");
    assert!(
        report["messages"][0]["message"]
            .as_str()
            .unwrap()
            .contains("Both moon.mod.json and moon.mod exist")
    );
    assert_eq!(report["summary"]["moon_warnings"], 1);
}

#[test]
fn test_moon_check_complete_json_captures_resolution_failure() {
    let dir = TestDir::new_empty();
    std::fs::write(
        dir.join("moon.mod.json"),
        r#"{
            "name": "test/root",
            "version": "0.1.0",
            "deps": {
                "this_user_should_not_exist/this_module_should_not_exist": "0.1.0"
            }
        }"#,
    )
    .unwrap();

    let report = parse_complete_json(moon_cmd(&dir).args(["check", "--json"]).assert().failure());

    assert_eq!(report["status"], "failure");
    assert_eq!(report["diagnostics"], serde_json::json!([]));
    assert!(
        report["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| {
                message["level"] == "warning"
                    && message["message"].as_str().unwrap().contains("moon update")
            })
    );
    assert!(
        report["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| {
                message["level"] == "error"
                    && message["message"]
                        .as_str()
                        .unwrap()
                        .contains("module was not found in the registry")
            })
    );
    assert_eq!(report["summary"]["moon_errors"], 1);
    assert_eq!(report["summary"]["moon_warnings"], 1);
}

#[test]
fn test_moon_check_complete_json_captures_postadd_output() {
    let report = parse_complete_json(check_json_with_postadd("moon --version").success());

    assert_eq!(report["status"], "success");
    assert!(
        report["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| {
                message["level"] == "info"
                    && message["message"]
                        .as_str()
                        .unwrap()
                        .contains("postadd script wrote to stdout")
            })
    );
}

#[test]
fn test_moon_check_complete_json_captures_postadd_failure() {
    let report =
        parse_complete_json(check_json_with_postadd("moon check --not-a-real-option").failure());

    assert_eq!(report["status"], "failure");
    assert!(
        report["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| {
                message["level"] == "error"
                    && message["message"]
                        .as_str()
                        .unwrap()
                        .contains("postadd script wrote to stderr")
                    && message["message"]
                        .as_str()
                        .unwrap()
                        .contains("--not-a-real-option")
            })
    );
}

#[test]
fn test_moon_check_complete_json_captures_workspace_override_warning() {
    let dir = TestDir::new("workspace_basic.in");
    let app_manifest = dir.join("app/moon.mod.json");
    let content = std::fs::read_to_string(&app_manifest)
        .unwrap()
        .replace(r#""alice/liba": "0.1.0""#, r#""alice/liba": "2.0.0""#);
    std::fs::write(app_manifest, content).unwrap();

    let report = parse_complete_json(moon_cmd(&dir).args(["check", "--json"]).assert().success());

    assert_eq!(report["status"], "success");
    assert!(
        report["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| {
                message["level"] == "warning"
                    && message["message"]
                        .as_str()
                        .unwrap()
                        .contains("overrides dependency requirement")
            })
    );
}

#[test]
fn test_moon_check_complete_json_captures_workspace_deprecation_once() {
    let dir = TestDir::new("workspace_basic.in");
    std::fs::write(
        dir.join("moon.work"),
        r#"members = [
  "./app",
  "./liba",
]
preferred_target = "wasm-gc"
"#,
    )
    .unwrap();

    let report = parse_complete_json(moon_cmd(&dir).args(["check", "--json"]).assert().success());
    let warnings = report["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| {
            message["message"]
                .as_str()
                .is_some_and(|message| message.contains("`preferred_target` in `moon.work`"))
        })
        .collect::<Vec<_>>();

    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0]["level"], "warning");
}

#[test]
fn test_moon_check_complete_json_captures_bin_dep_unstable_prebuild_config_failure() {
    let top_dir = TestDir::new("prebuild_config_script/check_skip_bin_dep.in");
    let dir = top_dir.join("user.in");
    std::fs::write(
        top_dir.join("author.in/build.js"),
        "console.error('BIN_DEP_JSON_SENTINEL')\nprocess.exit(1)\n",
    )
    .unwrap();

    let report = parse_complete_json(moon_cmd(&dir).args(["check", "--json"]).assert().failure());

    assert_eq!(report["status"], "failure");
    assert!(
        report["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| {
                message["level"] == "error"
                    && message["message"]
                        .as_str()
                        .unwrap()
                        .contains("BIN_DEP_JSON_SENTINEL")
            })
    );
}

#[test]
fn test_moon_check_complete_json_runs_bin_dep_unstable_prebuild_config() {
    let top_dir = TestDir::new("prebuild_config_script/check_skip_bin_dep.in");
    let dir = top_dir.join("user.in");
    let generated_stub = top_dir.join("author.in/src/main/generated_stub.c");
    std::fs::write(
        top_dir.join("author.in/moon.work"),
        "members = [\".\"]\npreferred_target = \"wasm-gc\"\n",
    )
    .unwrap();
    assert!(!generated_stub.exists());

    let report = parse_complete_json(
        moon_cmd(&dir)
            .args(["check", "--json", "--verbose"])
            .assert()
            .success(),
    );

    assert_eq!(report["status"], "success");
    assert_eq!(report["summary"]["moon_warnings"], 1);
    assert!(
        report["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| {
                message["level"] == "warning"
                    && message["message"]
                        .as_str()
                        .unwrap()
                        .contains("`bin-deps` is deprecated")
            })
    );
    assert!(
        report["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| {
                message["level"] == "info"
                    && message["message"]
                        .as_str()
                        .unwrap()
                        .contains("preferred_target")
            })
    );
    assert!(generated_stub.exists());
}

#[test]
fn test_moon_check_complete_json_rejects_other_output_modes() {
    let dir = TestDir::new_empty();
    let report = parse_complete_json(
        moon_cmd(&dir)
            .args(["check", "--json", "--output-json"])
            .assert()
            .code(2),
    );

    assert_eq!(report["status"], "failure");
    assert_eq!(report["messages"][0]["level"], "error");
    assert!(
        report["messages"][0]["message"]
            .as_str()
            .unwrap()
            .contains("--output-json")
    );
}

#[test]
fn test_moon_check_complete_json_starts_after_argument_parsing() {
    let dir = TestDir::new_empty();
    let help = moon_cmd(&dir)
        .args(["check", "--json", "--help"])
        .assert()
        .success()
        .stderr_eq("");
    let help = String::from_utf8_lossy(&help.get_output().stdout);
    assert!(
        help.contains("Usage:") && help.contains("--json"),
        "--help should keep Clap's human-readable output"
    );

    let error = moon_cmd(&dir)
        .args(["check", "--json", "--not-a-real-option"])
        .assert()
        .code(2)
        .stdout_eq("");
    assert!(
        String::from_utf8_lossy(&error.get_output().stderr).contains("unexpected argument"),
        "argument errors should keep Clap's native stderr output"
    );
}

#[test]
fn test_moon_check_json_output() {
    let dir = TestDir::new("warns/deny_warn");

    check(
        get_stdout(
            &dir,
            ["check", "--output-json", "--sort-input", "-j1", "-q"],
        ),
        expect![[r#"
            {"$message_type":"diagnostic","level":"warning","error_code":2,"path":"$ROOT/lib/hello.mbt","loc":"4:7-4:8","message":"Warning (unused_value): Unused variable 'a'","context":"3 |fn _a() -> Unit {/n4 |  let a = 1;/n5 |  // 中文中文中文中文中文中文/n"}
            {"$message_type":"diagnostic","level":"warning","error_code":2,"path":"$ROOT/lib/hello.mbt","loc":"11:7-11:9","message":"Warning (unused_value): Unused variable '中文'","context":"10 |  // 🤣😭🤣😭🤣😭🤣😭🤣😭/n11 |  let 中文 = 2/n12 |  let 🤣😭🤣😭🤣 = 2/n"}
            {"$message_type":"diagnostic","level":"warning","error_code":2,"path":"$ROOT/lib/hello.mbt","loc":"12:7-12:12","message":"Warning (unused_value): Unused variable '🤣😭🤣😭🤣'","context":"11 |  let 中文 = 2/n12 |  let 🤣😭🤣😭🤣 = 2/n13 |  alert_1();/n"}
            {"$message_type":"diagnostic","level":"warning","error_code":2,"path":"$ROOT/main/main.mbt","loc":"2:7-2:8","message":"Warning (unused_value): Unused variable 'a'","context":"1 |fn main {/n2 |  let a = 0/n3 |  @lib.hello()/n"}
        "#]],
    );
}

#[test]
fn test_moon_check_rendered_output() {
    let dir = TestDir::new("warns/deny_warn");

    check(
        get_stderr(&dir, ["check", "--sort-input", "-j1", "-q"]),
        expect![[r#"
            Warning: [0002]
               ╭─[ $ROOT/lib/hello.mbt:4:7 ]
               │
             4 │   let a = 1;
               │       ┬  
               │       ╰── Warning (unused_value): Unused variable 'a'
            ───╯
            Warning: [0002]
                ╭─[ $ROOT/lib/hello.mbt:11:7 ]
                │
             11 │   let 中文 = 2
                │       ──┬─  
                │         ╰─── Warning (unused_value): Unused variable '中文'
            ────╯
            Warning: [0002]
                ╭─[ $ROOT/lib/hello.mbt:12:7 ]
                │
             12 │   let 🤣😭🤣😭🤣 = 2
                │       ────┬─────  
                │           ╰─────── Warning (unused_value): Unused variable '🤣😭🤣😭🤣'
            ────╯
            Warning: [0002]
               ╭─[ $ROOT/main/main.mbt:2:7 ]
               │
             2 │   let a = 0
               │       ┬  
               │       ╰── Warning (unused_value): Unused variable 'a'
            ───╯
        "#]],
    );
}

#[test]
fn test_moon_check_raw_output() {
    let dir = TestDir::new("warns/deny_warn");

    check(
        get_stdout(&dir, ["check", "--no-render", "--sort-input", "-j1", "-q"]),
        expect![[r#"
            $ROOT/lib/hello.mbt:4:7-4:8 [E0002] Warning (unused_value): Unused variable 'a'
            $ROOT/lib/hello.mbt:11:7-11:9 [E0002] Warning (unused_value): Unused variable '中文'
            $ROOT/lib/hello.mbt:12:7-12:12 [E0002] Warning (unused_value): Unused variable '🤣😭🤣😭🤣'
            $ROOT/main/main.mbt:2:7-2:8 [E0002] Warning (unused_value): Unused variable 'a'
        "#]],
    );
}
