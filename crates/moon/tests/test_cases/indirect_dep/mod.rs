use super::*;

fn normalize_all_pkgs_json(dir: &impl AsRef<std::path::Path>, json_path: &Path) -> String {
    let json_content = std::fs::read_to_string(json_path).unwrap();
    replace_dir(&json_content, dir)
}

#[test]
fn test_all_pkgs() {
    let dir = TestDir::new("indirect_dep.in/indirect_dep1");

    // check
    let _ = get_stdout(&dir, ["clean"]);
    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
        Finished. moon: ran 10 tasks, now up to date
    "#]],
    );
    let all_pkgs_path = dir.join("_build/wasm-gc/debug/check/all_pkgs.json");
    let all_pkgs_json = normalize_all_pkgs_json(&dir, &all_pkgs_path);
    expect_file!["check_all_pkgs.json"].assert_eq(&all_pkgs_json);

    // build
    let _ = get_stdout(&dir, ["clean"]);
    check(
        get_stderr(&dir, ["build"]),
        expect![[r#"
            Finished. moon: ran 7 tasks, now up to date
        "#]],
    );
    let all_pkgs_path = dir.join("_build/wasm-gc/debug/build/all_pkgs.json");
    let all_pkgs_json = normalize_all_pkgs_json(&dir, &all_pkgs_path);
    expect_file!["build_all_pkgs.json"].assert_eq(&all_pkgs_json);

    // run
    let _ = get_stdout(&dir, ["clean"]);
    check(
        get_stdout(&dir, ["run", "cmd/main"]),
        expect![[r#"
        42
        42
    "#]],
    );
    let all_pkgs_path = dir.join("_build/wasm-gc/debug/build/all_pkgs.json");
    let all_pkgs_json = normalize_all_pkgs_json(&dir, &all_pkgs_path);
    expect_file!["run_all_pkgs.json"].assert_eq(&all_pkgs_json);

    // test
    let _ = get_stdout(&dir, ["clean"]);
    check(
        get_stdout(&dir, ["test"]),
        expect![[r#"
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
    let all_pkgs_path = dir.join("_build/wasm-gc/debug/test/all_pkgs.json");
    let all_pkgs_json = normalize_all_pkgs_json(&dir, &all_pkgs_path);
    expect_file!["test_all_pkgs.json"].assert_eq(&all_pkgs_json);

    // info
    let _ = get_stdout(&dir, ["clean"]);
    check(
        get_stderr(&dir, ["info"]),
        expect![[r#"
            Finished. moon: ran 10 tasks, now up to date
        "#]],
    );
    let all_pkgs_path = dir.join("_build/wasm-gc/debug/check/all_pkgs.json");
    let all_pkgs_json = normalize_all_pkgs_json(&dir, &all_pkgs_path);
    expect_file!["info_all_pkgs.json"].assert_eq(&all_pkgs_json);
}

#[test]
fn test_indirect_dep_bundle() {
    let dir = TestDir::new("indirect_dep.in/indirect_dep2");
    // bundle
    let _ = get_stdout(&dir, ["clean"]);
    check(
        get_stderr(&dir, ["bundle"]),
        expect![[r#"
            Finished. moon: ran 7 tasks, now up to date
        "#]],
    );
    let all_pkgs_path = dir.join("_build/wasm-gc/release/bundle/all_pkgs.json");
    let all_pkgs_json = normalize_all_pkgs_json(&dir, &all_pkgs_path);
    expect_file!["bundle_all_pkgs.json"].assert_eq(&all_pkgs_json);
}
