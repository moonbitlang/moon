use super::*;
use moonutil::common::BUILD_DIR;

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
        dir.join(BUILD_DIR)
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
            moonc build-package ./main/main.mbt -o ./_build/wasm/debug/build/main/main.core -pkg hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm/release/bundle' -i '$MOON_HOME/lib/core/target/wasm/release/bundle/prelude/prelude.mi:prelude' -pkg-sources hello/main:./main -target wasm -g -O0 -workspace-path . -all-pkgs ./_build/wasm/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/wasm/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm/release/bundle/core.core' ./_build/wasm/debug/build/main/main.core -main hello/main -o ./_build/wasm/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm -g -O0
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
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/build/main/main.core -main hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
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
            moonc build-package ./main/main.mbt -o ./_build/js/debug/build/main/main.core -pkg hello/main -is-main -std-path '$MOON_HOME/lib/core/target/js/release/bundle' -i '$MOON_HOME/lib/core/target/js/release/bundle/prelude/prelude.mi:prelude' -pkg-sources hello/main:./main -target js -g -O0 -source-map -workspace-path . -all-pkgs ./_build/js/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/js/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/js/release/bundle/core.core' ./_build/js/debug/build/main/main.core -main hello/main -o ./_build/js/debug/build/main/main.js -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target js -g -O0 -source-map
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

/// This test ensures that paths with non-ASCII names are handled correctly,
/// especially on Windows where codepages can cause issues.
///
/// See: https://github.com/moonbitlang/moon/issues/620
#[test]
fn test_non_ascii_path_names() {
    let template_dir = TestDir::new("hello");
    let unicode_dir = TestDir::new_empty();
    let unicode_dir = unicode_dir.join("中文路径");
    std::fs::create_dir_all(&unicode_dir).unwrap();

    // Copy recursively into the non-ASCII path
    crate::util::copy(template_dir.as_ref(), &unicode_dir).unwrap();

    // Run a command to ensure it works
    let output = get_stdout(&unicode_dir, ["run", "main"]);
    assert_eq!(output.trim(), "Hello, world!");
}
