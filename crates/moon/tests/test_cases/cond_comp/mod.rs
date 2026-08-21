use super::*;

#[test]
fn test_cond_comp() {
    run_moon_cmdtest("cond_comp.in");
}

#[test]
fn test_packages_json_is_profile_projection() {
    let dir = TestDir::new("cond_comp.in");

    moon_cmd(&dir)
        .args(["check", "--target", "js", "--release"])
        .assert()
        .success();

    let packages_json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(scoped_packages_json_path(&dir, "js", "release")).unwrap(),
    )
    .unwrap();
    let lib = packages_json["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| package["rel"] == "lib")
        .unwrap();
    let files = lib["files"].as_object().unwrap();
    let has_file = |name: &str| files.keys().any(|path| path.ends_with(name));

    assert!(has_file("js_and_release.mbt"));
    assert!(!has_file("only_debug.mbt"));
    assert!(!has_file("wasm_release_or_js_debug.mbt"));
}
