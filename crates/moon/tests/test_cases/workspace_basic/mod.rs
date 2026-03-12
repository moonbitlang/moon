use super::*;

#[test]
fn test_workspace_build_and_check() {
    let dir = TestDir::new("workspace_basic.in");

    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./liba/src/lib/lib.mbt -o ./_build/wasm-gc/debug/build/alice/liba/lib/lib.core -pkg alice/liba/lib -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources alice/liba/lib:./liba/src/lib -target wasm-gc -g -O0 -source-map -workspace-path ./liba -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./app/src/main/main.mbt -o ./_build/wasm-gc/debug/build/alice/app/main/main.core -pkg alice/app/main -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/build/alice/liba/lib/lib.mi:lib -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources alice/app/main:./app/src/main -target wasm-gc -g -O0 -source-map -workspace-path ./app -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/build/alice/liba/lib/lib.core ./_build/wasm-gc/debug/build/alice/app/main/main.core -main alice/app/main -o ./_build/wasm-gc/debug/build/alice/app/main/main.wasm -pkg-config-path ./app/src/main/moon.pkg.json -pkg-sources alice/liba/lib:./liba/src/lib -pkg-sources alice/app/main:./app/src/main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
        "#]],
    );

    let stderr = get_stderr(&dir, ["check", "--sort-input"]);
    assert!(stderr.contains("Finished. moon: ran "));

    let metadata = std::fs::read_to_string(dir.join("_build/packages.json")).unwrap();
    let metadata = replace_dir(&metadata, &dir);
    let metadata: serde_json::Value = serde_json::from_str(&metadata).unwrap();

    assert_eq!(metadata["source_dir"], "$ROOT");
    assert_eq!(metadata["name"], "workspace");
    assert_eq!(metadata["deps"], serde_json::json!(["alice/liba"]));
    assert_eq!(metadata["backend"], "wasm-gc");
    assert_eq!(metadata["opt_level"], "release");
    assert_eq!(metadata["source"], serde_json::Value::Null);
    assert_eq!(metadata.get("workspace"), None);

    let packages = metadata["packages"].as_array().unwrap();
    let artifact_for = |root: &str, rel: &str| {
        packages
            .iter()
            .find(|pkg| pkg["root"] == root && pkg["rel"] == rel)
            .unwrap()["artifact"]
            .as_str()
            .unwrap()
            .to_owned()
    };

    assert_eq!(
        artifact_for("alice/app", "main"),
        "$ROOT/_build/wasm-gc/release/check/alice/app/main/main.mi"
    );
    assert_eq!(
        artifact_for("alice/liba", "lib"),
        "$ROOT/_build/wasm-gc/release/check/alice/liba/lib/lib.mi"
    );
}
