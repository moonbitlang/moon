use super::*;

fn normalize_all_pkgs_json(dir: &impl AsRef<std::path::Path>, json_path: &Path) -> String {
    let path_str = dunce::canonicalize(dir)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let mut all_pkgs: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(json_path).unwrap()).unwrap();
    let packages = all_pkgs["packages"].as_array_mut().unwrap();
    let mut actual_core_packages = packages
        .iter()
        .filter(|package| package["root"] == "moonbitlang/core")
        .map(|package| {
            (
                package["rel"].as_str().unwrap().to_owned(),
                PathBuf::from(package["artifact"].as_str().unwrap()),
            )
        })
        .collect::<Vec<_>>();
    actual_core_packages.sort();
    assert_eq!(
        actual_core_packages,
        core_package_interfaces(TargetBackend::WasmGC),
        "all_pkgs.json does not match the installed core package interfaces"
    );

    // Standard library membership follows the installed toolchain and is
    // unrelated to the indirect project dependencies covered here.
    packages.retain(|package| package["root"] != "moonbitlang/core");
    let json_content = serde_json::to_string_pretty(&all_pkgs).unwrap() + "\n";

    // Normalize Windows paths: replace backslashes with forward slashes
    // In JSON, Windows paths are escaped (e.g., "C:\\Users\\..."), so we replace "\\" with "/"
    // For the canonical path, we replace single "\" with "/"
    let normalized_path = path_str.replace('\\', "/");
    let normalized_json = json_content.replace("\\\\", "/");

    // Replace the project path with $ROOT
    let normalized_json = normalized_json.replace(&normalized_path, "$ROOT");

    // Replace the MOON_HOME path with $MOON_HOME
    normalized_json.replace(
        &moonutil::toolchain::home()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
        "$MOON_HOME",
    )
}

#[test]
fn test_all_pkgs() {
    let dir = TestDir::new("indirect_dep.in/indirect_dep1");

    // check
    let _ = get_stdout(&dir, ["clean"]);
    check(
        get_stderr(&dir, ["check", "--target", "wasm-gc"]),
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
        get_stderr(&dir, ["build", "--target", "wasm-gc"]),
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
        get_stdout(&dir, ["run", "--target", "wasm-gc", "cmd/main"]),
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
        get_stdout(&dir, ["test", "--target", "wasm-gc"]),
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
        get_stderr(&dir, ["info", "--target", "wasm-gc"]),
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
        get_stderr(&dir, ["bundle", "--target", "wasm-gc"]),
        expect![[r#"
            Finished. moon: ran 7 tasks, now up to date
        "#]],
    );
    let all_pkgs_path = dir.join("_build/wasm-gc/release/bundle/all_pkgs.json");
    let all_pkgs_json = normalize_all_pkgs_json(&dir, &all_pkgs_path);
    expect_file!["bundle_all_pkgs.json"].assert_eq(&all_pkgs_json);
}
