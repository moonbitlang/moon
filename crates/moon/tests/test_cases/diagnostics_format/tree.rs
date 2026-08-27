use super::json_command_with_postadd;
use crate::{TestDir, moon_cmd};

#[test]
fn test_moon_tree_package_json_captures_postadd_output() {
    json_command_with_postadd(&["tree", "--package", "--json"], "moon --version")
        .success()
        .stderr_eq("")
        .stdout_eq(snapbox::str![[r#"
{"version":2,"status":"success","error":null,"root":[0],"nodes":[{"module":"test/root","version":"0.1.0","source":{"kind":"local","path":"[..]"},"rel":"lib"}],"edges":[],"logs":[{"level":"info","message":"Using cached testuser/postadd@1.0.0"},{"level":"info","message":"postadd script wrote to stdout:/nmoon [..]/n/nFeature flags enabled: rr_moon_mod,rr_moon_pkg"}]}

"#]]);
}

#[test]
fn test_moon_tree_package_json_captures_postadd_failure() {
    json_command_with_postadd(
        &["tree", "--package", "--json"],
        "moon check --not-a-real-option",
    )
    .failure()
    .stderr_eq("")
    .stdout_eq(snapbox::str![[r#"
{"version":2,"status":"failure","error":"Failed to resolve the module dependency graph: When installing packages: failed to execute postadd script in [..],/ncommand: moon","root":[],"nodes":[],"edges":[],"logs":[{"level":"info","message":"Using cached testuser/postadd@1.0.0"},{"level":"error","message":"postadd script wrote to stderr:/nerror: unexpected argument '--not-a-real-option' found/n/n[..]Usage: moon check [OPTIONS] [PATH].../n/nFor more information, try '--help'."}]}

"#]]);
}

#[test]
fn test_moon_tree_package_json_aggregates_target_kinds_per_alias() {
    let dir = TestDir::new_empty();
    std::fs::create_dir_all(dir.join("src/root")).unwrap();
    std::fs::create_dir_all(dir.join("src/dep")).unwrap();
    std::fs::write(
        dir.join("moon.mod.json"),
        r#"{
            "name": "test/edges",
            "version": "0.1.0",
            "source": "src"
        }"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("src/root/moon.pkg.json"),
        r#"{
            "import": [{ "path": "test/edges/dep", "alias": "shared" }],
            "wbtest-import": [{ "path": "test/edges/dep", "alias": "shared" }],
            "test-import": [{ "path": "test/edges/dep", "alias": "blackbox" }]
        }"#,
    )
    .unwrap();
    std::fs::write(dir.join("src/dep/moon.pkg.json"), "{}").unwrap();
    std::fs::write(dir.join("src/root/root.mbt"), "fn root() -> Unit { () }").unwrap();
    std::fs::write(dir.join("src/dep/dep.mbt"), "pub fn dep() -> Unit { () }").unwrap();

    moon_cmd(&dir)
        .args(["tree", "--package", "--json"])
        .assert()
        .success()
        .stderr_eq("")
        .stdout_eq(snapbox::str![[r#"
{"version":2,"status":"success","error":null,"root":[0,1],"nodes":[{"module":"test/edges","version":"0.1.0","source":{"kind":"local","path":"[..]"},"rel":"dep"},{"module":"test/edges","version":"0.1.0","source":{"kind":"local","path":"[..]"},"rel":"root"}],"edges":[{"from":1,"to":0,"alias":"blackbox","kinds":["blackbox-test"]},{"from":1,"to":0,"alias":"shared","kinds":["source","whitebox-test"]}],"logs":[]}

"#]]);
}
