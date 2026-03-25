use super::*;

fn normalized_path(value: &serde_json::Value) -> &str {
    value.as_str().unwrap()
}

#[test]
fn implement_third_party1() {
    let dir = TestDir::new("virtual_pkg2.in/p");
    let check_graph = dir.join("check_graph.json");
    snap_dry_run_graph(&dir, ["check", ".", "--dry-run"], &check_graph);
    compare_graphs(&check_graph, expect_file!["./check_graph.jsonl"]);

    let s = get_stderr(&dir, ["check", "."]);
    check(
        s,
        expect![[r#"
            Finished. moon: ran 2 tasks, now up to date
        "#]],
    );

    let packages_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("_build/packages.json")).unwrap())
            .unwrap();
    let packages = packages_json["packages"].as_array().unwrap();

    let implementor = packages
        .iter()
        .find(|pkg| pkg["root"] == "username/p")
        .unwrap();
    assert!(
        normalized_path(&implementor["artifact"])
            .replace('\\', "/")
            .ends_with("/check/p.impl.mi")
    );

    let virtual_pkg = packages
        .iter()
        .find(|pkg| pkg["root"] == "username/v")
        .unwrap();
    assert!(
        normalized_path(&virtual_pkg["artifact"])
            .replace('\\', "/")
            .ends_with("/check/.mooncakes/username/v/v.mi")
    );

    let all_pkgs_json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("_build/wasm-gc/debug/check/all_pkgs.json")).unwrap(),
    )
    .unwrap();
    let all_pkgs = all_pkgs_json["packages"].as_array().unwrap();

    let implementor = all_pkgs
        .iter()
        .find(|pkg| pkg["root"] == "username/p")
        .unwrap();
    assert!(
        normalized_path(&implementor["artifact"])
            .replace('\\', "/")
            .ends_with("/check/p.impl.mi")
    );

    let virtual_pkg = all_pkgs
        .iter()
        .find(|pkg| pkg["root"] == "username/v")
        .unwrap();
    assert!(
        normalized_path(&virtual_pkg["artifact"])
            .replace('\\', "/")
            .ends_with("/check/.mooncakes/username/v/v.mi")
    );
}

#[test]
fn implement_third_party2() {
    let dir = TestDir::new("virtual_pkg2.in/p");
    let build_graph = dir.join("build_graph.json");
    snap_dry_run_graph(&dir, ["build", "--dry-run"], &build_graph);
    compare_graphs(&build_graph, expect_file!["./build_graph.jsonl"]);

    let s = get_stderr(&dir, ["build"]);
    check(
        s,
        expect![[r#"
        Finished. moon: ran 2 tasks, now up to date
    "#]],
    );
}
