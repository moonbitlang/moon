use super::json_command_with_postadd;

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
