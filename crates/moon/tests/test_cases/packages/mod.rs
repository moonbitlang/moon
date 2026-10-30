use super::*;

#[test]
fn test_empty_name() {
    let dir = TestDir::new("packages/empty_name");
    let err = get_err_stderr(&dir, ["check"]);
    println!("Error output:\n{}", err);
    assert!(err.contains("`name` should not be empty"));
}

#[test]
fn test_error_duplicate_alias() {
    let dir = TestDir::new("packages/error_duplicate_alias");
    let out = get_err_stderr(&dir, ["check"]);
    println!("Error output:\n{}", out);
    assert!(out.contains("Duplicate alias `lib`") || out.contains("Conflicting import alias"));
}

#[test]
fn test_import_all_with_named_alias_dry_run() {
    let dir = TestDir::new("packages/import_all_with_named_alias");
    let out = get_stdout(&dir, ["build", "--target", "wasm-gc", "--dry-run"]);
    assert!(
        out.contains("./_build/wasm-gc/debug/build/dep/dep.mi:*dep"),
        "{out}"
    );
}

#[test]
fn test_import_all_with_named_alias_conflicts_with_alias() {
    let dir = TestDir::new("packages/import_all_with_named_alias_conflict");
    let out = get_err_stderr(&dir, ["check", "--dry-run"]);
    assert!(out.contains("same alias 'dep'"), "{out}");
}

#[test]
fn test_import_all_with_named_alias_blackbox_self_alias_rewrite() {
    let dir = TestDir::new("packages/import_all_with_named_alias");
    let out = get_stdout(&dir, ["test", "lib", "--target", "wasm-gc", "--dry-run"]);
    assert!(
        out.contains(
            "./_build/wasm-gc/debug/test/dep/dep.mi:*test/import_all_with_named_alias/dep"
        ),
        "{out}"
    );
}

#[test]
fn test_core_order() {
    let dir = TestDir::new("packages/core_order");
    check(
        get_stdout(&dir, ["build", "--target", "wasm-gc", "--dry-run"]),
        expect![[r#"
            moonc build-package ./T/t.mbt -o ./_build/wasm-gc/debug/build/T/T.core -pkg lijunchen/hello/T -pkg-type library -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources lijunchen/hello/T:./T -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./B/b.mbt -o ./_build/wasm-gc/debug/build/B/B.core -pkg lijunchen/hello/B -pkg-type library -i ./_build/wasm-gc/debug/build/T/T.mi:T -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources lijunchen/hello/B:./B -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./A/a.mbt -o ./_build/wasm-gc/debug/build/A/A.core -pkg lijunchen/hello/A -pkg-type library -i ./_build/wasm-gc/debug/build/T/T.mi:T -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources lijunchen/hello/A:./A -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg lijunchen/hello/main -pkg-type executable -i ./_build/wasm-gc/debug/build/A/A.mi:A -i ./_build/wasm-gc/debug/build/B/B.mi:B -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources lijunchen/hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/builtin/builtin.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/char/char.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/float/float.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/int16/int16.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/debug/debug.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/buffer/buffer.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/double/double.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/test/test.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/uint64/uint64.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/internal/strconv/strconv.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/v128/v128.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/json/json.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/uint/uint.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/uint16/uint16.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bigint/bigint.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/internal/regex_engine/shared_types/byte_set/byte_set.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/internal/regex_engine/shared_types/shared_types.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/internal/regex_engine/ast/ast.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/internal/regex_engine/symbol_map/symbol_map.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/hashmap/hashmap.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/internal/regex_engine/regex_engine.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/set/set.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/internal/regex_parser/regex_parser.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/bytes.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/lazy/lazy.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/ref/ref.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_engine/shared_types/rechar_set/rechar_set.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_engine/shared_types/shared_types.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_engine/ast/ast.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/sorted_set/sorted_set.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_engine/symbol_map/symbol_map.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_engine/regex_engine.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_parser/regex_parser.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/string.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.core' ./_build/wasm-gc/debug/build/T/T.core ./_build/wasm-gc/debug/build/A/A.core ./_build/wasm-gc/debug/build/B/B.core ./_build/wasm-gc/debug/build/main/main.core -main lijunchen/hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg -pkg-sources lijunchen/hello/T:./T -pkg-sources lijunchen/hello/A:./A -pkg-sources lijunchen/hello/B:./B -pkg-sources lijunchen/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
        "#]],
    );
}
