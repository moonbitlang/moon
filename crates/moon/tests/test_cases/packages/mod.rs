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
fn test_core_order() {
    let dir = TestDir::new("packages/core_order");
    check(
        get_stdout(&dir, ["build", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./T/t.mbt -o ./_build/wasm-gc/debug/build/T/T.core -pkg lijunchen/hello/T -pkg-sources lijunchen/hello/T:./T -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./B/b.mbt -o ./_build/wasm-gc/debug/build/B/B.core -pkg lijunchen/hello/B -i ./_build/wasm-gc/debug/build/T/T.mi:T -pkg-sources lijunchen/hello/B:./B -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./A/a.mbt -o ./_build/wasm-gc/debug/build/A/A.core -pkg lijunchen/hello/A -i ./_build/wasm-gc/debug/build/T/T.mi:T -pkg-sources lijunchen/hello/A:./A -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg lijunchen/hello/main -is-main -i ./_build/wasm-gc/debug/build/A/A.mi:A -i ./_build/wasm-gc/debug/build/B/B.mi:B -pkg-sources lijunchen/hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/debug/build/T/T.core ./_build/wasm-gc/debug/build/A/A.core ./_build/wasm-gc/debug/build/B/B.core ./_build/wasm-gc/debug/build/main/main.core -main lijunchen/hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources lijunchen/hello/T:./T -pkg-sources lijunchen/hello/A:./A -pkg-sources lijunchen/hello/B:./B -pkg-sources lijunchen/hello/main:./main -target wasm-gc -g -O0 -source-map
        "#]],
    );
}
