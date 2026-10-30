use super::*;

fn tree_project() -> TestDir {
    let dir = TestDir::new_empty();
    std::fs::write(dir.join("moon.mod"), "name = \"test/tree\"\n").unwrap();
    for (package, manifest) in [("a", "import { \"test/tree/b\" }\n"), ("b", "")] {
        std::fs::create_dir(dir.join(package)).unwrap();
        std::fs::write(dir.join(format!("{package}/moon.pkg")), manifest).unwrap();
    }
    dir
}

#[test]
fn test_package_tree_accepts_each_target_backend() {
    let dir = tree_project();
    for target in ["wasm", "wasm-gc", "js", "native", "llvm"] {
        moon_cmd(&dir)
            .args(["tree", "--package", "--target", target])
            .assert()
            .success()
            .stderr_eq("")
            .stdout_eq(snapbox::str![[r#"
test/tree@0.0.0 (local [..]) [a]:
└─ b -> test/tree@0.0.0 (local [..]) [b]

test/tree@0.0.0 (local [..]) [b]:
  (no dependencies)

"#]]);
    }
}

#[test]
fn test_package_tree_json_uses_module_default_and_explicit_override() {
    let dir = tree_project();
    std::fs::write(
        dir.join("moon.mod"),
        "name = \"test/tree\"\npreferred_target = \"native\"\n",
    )
    .unwrap();
    moon_cmd(&dir)
        .args(["tree", "--package", "--target", "js", "--json"])
        .assert()
        .success()
        .stderr_eq("")
        .stdout_eq(snapbox::str![[r#"
{"version":3,"status":"success","error":null,"target":"js","root":[0,1],"nodes":[{"module":"test/tree","version":"0.0.0","source":{"kind":"local","path":"[..]"},"rel":"a"},{"module":"test/tree","version":"0.0.0","source":{"kind":"local","path":"[..]"},"rel":"b"}],"edges":[{"from":0,"to":1,"alias":"b","kinds":["source"]}],"logs":[]}

"#]]);
    moon_cmd(&dir)
        .args(["tree", "--package", "--json"])
        .assert()
        .success()
        .stderr_eq("")
        .stdout_eq(snapbox::str![[r#"
{"version":3,"status":"success","error":null,"target":"native","root":[0,1],"nodes":[{"module":"test/tree","version":"0.0.0","source":{"kind":"local","path":"[..]"},"rel":"a"},{"module":"test/tree","version":"0.0.0","source":{"kind":"local","path":"[..]"},"rel":"b"}],"edges":[{"from":0,"to":1,"alias":"b","kinds":["source"]}],"logs":[]}

"#]]);
}

#[test]
fn test_package_tree_uses_selected_workspace_members_preferred_target() {
    let dir = TestDir::new("workspace_conflicting_preferred_targets.in");
    for (member, target) in [("js_preferred", "js"), ("native_preferred", "native")] {
        let assert = moon_cmd(&dir)
            .args(["-C", member, "tree", "--package", "--json"])
            .assert()
            .success()
            .stderr_eq("");
        let report: serde_json::Value =
            serde_json::from_slice(&assert.get_output().stdout).unwrap();
        assert_eq!(report["target"], target);
    }
}

#[test]
fn test_package_tree_json_error_keeps_requested_backend() {
    let dir = tree_project();
    std::fs::write(dir.join("a/moon.pkg"), "import { \"test/tree/missing\" }\n").unwrap();
    moon_cmd(&dir)
        .args(["tree", "--package", "--target", "native", "--json"])
        .assert()
        .failure()
        .stderr_eq("")
        .stdout_eq(snapbox::str![[r#"
{"version":3,"status":"failure","error":"[..]Cannot find import 'test/tree/missing' in test/tree/a[..]","target":"native","root":[],"nodes":[],"edges":[],"logs":[]}

"#]]);
}
