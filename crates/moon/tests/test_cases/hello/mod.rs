use super::*;

#[test]
fn test_hello() {
    let dir = TestDir::new("hello");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
    check(
        dir.join("target")
            .join("common")
            .join(".moon-lock")
            .exists()
            .to_string(),
        expect!["false"],
    );
}

#[test]
fn test_source_map() {
    let dir = TestDir::new("hello");

    // no -source-map in wasm backend
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "wasm",
                "--debug",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./main/main.mbt -o ./target/wasm/debug/build/main/main.core -pkg hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -pkg-sources hello/main:./main -target wasm -g -O0 -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm/release/bundle/core.core ./target/wasm/debug/build/main/main.core -main hello/main -o ./target/wasm/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm -g -O0
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "wasm-gc",
                "--debug",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "js",
                "--debug",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./main/main.mbt -o ./target/js/debug/build/main/main.core -pkg hello/main -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources hello/main:./main -target js -g -O0 -source-map -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/js/release/bundle/core.core ./target/js/debug/build/main/main.core -main hello/main -o ./target/js/debug/build/main/main.js -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target js -g -O0 -source-map
        "#]],
    );
}

#[test]
fn test_find_ancestor_with_mod() {
    let dir = TestDir::new("hello");

    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );

    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
}

#[test]
fn test_preferred_target() {
    use serde_json_lenient::Value;
    let dir = TestDir::new("hello");

    // Replace the preferred backend in moon.mod.json
    let mod_json_path = dir.join("moon.mod.json");
    let mut mod_json: Value =
        serde_json_lenient::from_slice(&std::fs::read(&mod_json_path).unwrap()).unwrap();

    // Helper function to test a specific target
    fn test_target(
        dir: &TestDir,
        mod_json_path: &std::path::Path,
        mod_json: &mut Value,
        target: &str,
    ) {
        mod_json["preferred-target"] = target.into();
        std::fs::write(
            mod_json_path,
            serde_json_lenient::to_string(mod_json).unwrap(),
        )
        .unwrap();
        let target_flag = format!("-target {target}");

        let build_output = get_stdout(dir, ["build", "--dry-run"]);
        let test_output = get_stdout(dir, ["test", "--dry-run"]);
        let check_output = get_stdout(dir, ["check", "--dry-run"]);

        assert!(
            build_output.contains(&target_flag),
            "build output doesn't contain '{target_flag}': {build_output:?}"
        );
        assert!(
            test_output.contains(&target_flag),
            "test output doesn't contain '{target_flag}': {test_output:?}"
        );
        assert!(
            check_output.contains(&target_flag),
            "check output doesn't contain '{target_flag}': {check_output:?}"
        );
    }

    // Test different target values
    test_target(&dir, &mod_json_path, &mut mod_json, "js");
    test_target(&dir, &mod_json_path, &mut mod_json, "wasm");
    test_target(&dir, &mod_json_path, &mut mod_json, "wasm-gc");
    test_target(&dir, &mod_json_path, &mut mod_json, "native");
}
