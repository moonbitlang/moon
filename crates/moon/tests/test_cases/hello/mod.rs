use super::*;
use moonutil::constants::BUILD_DIR;

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
            moonc build-package ./main/main.mbt -o ./_build/wasm/debug/build/main/main.core -pkg hello/main -pkg-type executable -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/prelude/prelude.mi:prelude' -pkg-sources hello/main:./main -target wasm -g -O0 -workspace-path . -all-pkgs ./_build/wasm/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm/release/bundle/builtin/builtin.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/char/char.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/float/float.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/int16/int16.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/debug/debug.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/buffer/buffer.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/double/double.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/test/test.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/uint64/uint64.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/internal/strconv/strconv.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/v128/v128.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/json/json.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/uint/uint.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/uint16/uint16.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/bigint/bigint.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/lazy/lazy.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/ref/ref.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/set/set.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/hashmap/hashmap.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/internal/regex_engine/shared_types/rechar_set/rechar_set.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/internal/regex_engine/shared_types/shared_types.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/string/internal/regex_engine/ast/ast.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/sorted_set/sorted_set.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/string/internal/regex_engine/symbol_map/symbol_map.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/string/internal/regex_engine/regex_engine.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/string/internal/regex_parser/regex_parser.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/string/string.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/prelude/prelude.core' ./_build/wasm/debug/build/main/main.core -main hello/main -o ./_build/wasm/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg -pkg-sources hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm -g -O0 -wasi
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
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg hello/main -pkg-type executable -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/builtin/builtin.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/char/char.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/float/float.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/int16/int16.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/debug/debug.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/buffer/buffer.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/double/double.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/test/test.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/uint64/uint64.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/internal/strconv/strconv.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/v128/v128.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/json/json.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/uint/uint.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/uint16/uint16.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bigint/bigint.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/lazy/lazy.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/ref/ref.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/set/set.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/hashmap/hashmap.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/internal/regex_engine/shared_types/rechar_set/rechar_set.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/internal/regex_engine/shared_types/shared_types.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_engine/ast/ast.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/sorted_set/sorted_set.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_engine/symbol_map/symbol_map.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_engine/regex_engine.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_parser/regex_parser.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/string.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.core' ./_build/wasm-gc/debug/build/main/main.core -main hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg -pkg-sources hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
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
            moonc build-package ./main/main.mbt -o ./_build/js/debug/build/main/main.core -pkg hello/main -pkg-type executable -i '$MOON_HOME/lib/core/_build/js/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/js/release/bundle/prelude/prelude.mi:prelude' -pkg-sources hello/main:./main -target js -g -O0 -source-map -workspace-path . -all-pkgs ./_build/js/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/js/release/bundle/builtin/builtin.core' '$MOON_HOME/lib/core/_build/js/release/bundle/char/char.core' '$MOON_HOME/lib/core/_build/js/release/bundle/float/float.core' '$MOON_HOME/lib/core/_build/js/release/bundle/int16/int16.core' '$MOON_HOME/lib/core/_build/js/release/bundle/debug/debug.core' '$MOON_HOME/lib/core/_build/js/release/bundle/buffer/buffer.core' '$MOON_HOME/lib/core/_build/js/release/bundle/double/double.core' '$MOON_HOME/lib/core/_build/js/release/bundle/test/test.core' '$MOON_HOME/lib/core/_build/js/release/bundle/uint64/uint64.core' '$MOON_HOME/lib/core/_build/js/release/bundle/internal/strconv/strconv.core' '$MOON_HOME/lib/core/_build/js/release/bundle/v128/v128.core' '$MOON_HOME/lib/core/_build/js/release/bundle/json/json.core' '$MOON_HOME/lib/core/_build/js/release/bundle/uint/uint.core' '$MOON_HOME/lib/core/_build/js/release/bundle/uint16/uint16.core' '$MOON_HOME/lib/core/_build/js/release/bundle/bigint/bigint.core' '$MOON_HOME/lib/core/_build/js/release/bundle/lazy/lazy.core' '$MOON_HOME/lib/core/_build/js/release/bundle/ref/ref.core' '$MOON_HOME/lib/core/_build/js/release/bundle/set/set.core' '$MOON_HOME/lib/core/_build/js/release/bundle/hashmap/hashmap.core' '$MOON_HOME/lib/core/_build/js/release/bundle/internal/regex_engine/shared_types/rechar_set/rechar_set.core' '$MOON_HOME/lib/core/_build/js/release/bundle/internal/regex_engine/shared_types/shared_types.core' '$MOON_HOME/lib/core/_build/js/release/bundle/string/internal/regex_engine/ast/ast.core' '$MOON_HOME/lib/core/_build/js/release/bundle/sorted_set/sorted_set.core' '$MOON_HOME/lib/core/_build/js/release/bundle/string/internal/regex_engine/symbol_map/symbol_map.core' '$MOON_HOME/lib/core/_build/js/release/bundle/string/internal/regex_engine/regex_engine.core' '$MOON_HOME/lib/core/_build/js/release/bundle/string/internal/regex_parser/regex_parser.core' '$MOON_HOME/lib/core/_build/js/release/bundle/string/string.core' '$MOON_HOME/lib/core/_build/js/release/bundle/prelude/prelude.core' ./_build/js/debug/build/main/main.core -main hello/main -o ./_build/js/debug/build/main/main.js -pkg-config-path ./main/moon.pkg -pkg-sources hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target js -g -O0 -source-map
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
    let dir = TestDir::new("hello");
    fn test_target(dir: &TestDir, target: &str) {
        std::fs::write(
            dir.join("moon.mod"),
            format!(
                r#"
name = "hello"
preferred_target = {target:?}
"#
            ),
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
    test_target(&dir, "js");
    test_target(&dir, "wasm");
    test_target(&dir, "wasm-gc");
    test_target(&dir, "native");
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
