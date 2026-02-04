use super::*;

fn normalize_all_pkgs_json(dir: &impl AsRef<std::path::Path>, json_path: &Path) -> String {
    let path_str = dunce::canonicalize(dir)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let json_content = std::fs::read_to_string(json_path).unwrap();

    // Normalize Windows paths: replace backslashes with forward slashes
    // In JSON, Windows paths are escaped (e.g., "C:\\Users\\..."), so we replace "\\" with "/"
    // For the canonical path, we replace single "\" with "/"
    let normalized_path = path_str.replace('\\', "/");
    let normalized_json = json_content.replace("\\\\", "/");

    // Replace the project path with $ROOT
    let normalized_json = normalized_json.replace(&normalized_path, "$ROOT");

    // Replace the MOON_HOME path with $MOON_HOME
    normalized_json.replace(
        &moonutil::moon_dir::home()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
        "$MOON_HOME",
    )
}

fn find_all_pkgs_json(dir: &impl AsRef<Path>, backend: &str, mode: &str) -> PathBuf {
    let debug = dir
        .as_ref()
        .join("target")
        .join(backend)
        .join("debug")
        .join(mode)
        .join("all_pkgs.json");
    if debug.exists() {
        return debug;
    }
    let release = dir
        .as_ref()
        .join("target")
        .join(backend)
        .join("release")
        .join(mode)
        .join("all_pkgs.json");
    if release.exists() {
        return release;
    }
    panic!("all_pkgs.json not found for backend={backend} mode={mode}");
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
    let all_pkgs_path = find_all_pkgs_json(&dir, "wasm-gc", "check");
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
    let all_pkgs_path = find_all_pkgs_json(&dir, "wasm-gc", "build");
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
    let all_pkgs_path = find_all_pkgs_json(&dir, "wasm-gc", "build");
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
    let all_pkgs_path = dir.join("target/wasm-gc/debug/test/all_pkgs.json");
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
    let all_pkgs_path = find_all_pkgs_json(&dir, "wasm-gc", "check");
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
    let all_pkgs_path = dir.join("target/wasm-gc/release/bundle/all_pkgs.json");
    let all_pkgs_json = normalize_all_pkgs_json(&dir, &all_pkgs_path);
    expect_file!["bundle_all_pkgs.json"].assert_eq(&all_pkgs_json);
}
