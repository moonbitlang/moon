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
    assert!(out.contains("Duplicate alias `lib`"));
}

#[test]
fn test_core_order() {
    let dir = TestDir::new("packages/core_order");
    check(
        get_stdout(&dir, ["build", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./T/t.mbt -o ./target/wasm-gc/release/build/T/T.core -pkg lijunchen/hello/T -pkg-sources lijunchen/hello/T:./T -target wasm-gc -workspace-path .
            moonc build-package ./B/b.mbt -o ./target/wasm-gc/release/build/B/B.core -pkg lijunchen/hello/B -i ./target/wasm-gc/release/build/T/T.mi:T -pkg-sources lijunchen/hello/B:./B -target wasm-gc -workspace-path .
            moonc build-package ./A/a.mbt -o ./target/wasm-gc/release/build/A/A.core -pkg lijunchen/hello/A -i ./target/wasm-gc/release/build/T/T.mi:T -pkg-sources lijunchen/hello/A:./A -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg lijunchen/hello/main -is-main -i ./target/wasm-gc/release/build/A/A.mi:A -i ./target/wasm-gc/release/build/B/B.mi:B -pkg-sources lijunchen/hello/main:./main -target wasm-gc -workspace-path .
            moonc link-core ./target/wasm-gc/release/build/T/T.core ./target/wasm-gc/release/build/A/A.core ./target/wasm-gc/release/build/B/B.core ./target/wasm-gc/release/build/main/main.core -main lijunchen/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources lijunchen/hello/T:./T -pkg-sources lijunchen/hello/A:./A -pkg-sources lijunchen/hello/B:./B -pkg-sources lijunchen/hello/main:./main -target wasm-gc
        "#]],
    );
}
