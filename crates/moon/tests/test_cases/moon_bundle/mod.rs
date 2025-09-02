use super::*;

#[test]
fn test_moon_bundle() {
    let dir = TestDir::new("moon_bundle");
    check(
        get_stdout(&dir, ["bundle", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package -o ./target/wasm-gc/release/bundle/prelude/prelude.core -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target wasm-gc
            moonc build-package ./Orphan/lib.mbt -o ./target/wasm-gc/release/bundle/Orphan/Orphan.core -pkg moonbitlang/core/Orphan -pkg-sources moonbitlang/core/Orphan:./Orphan -target wasm-gc
            moonc build-package ./A/lib.mbt -o ./target/wasm-gc/release/bundle/A/A.core -pkg moonbitlang/core/A -pkg-sources moonbitlang/core/A:./A -target wasm-gc
            moonc build-package ./C/lib.mbt -o ./target/wasm-gc/release/bundle/C/C.core -pkg moonbitlang/core/C -i ./target/wasm-gc/release/bundle/A/A.mi:A -pkg-sources moonbitlang/core/C:./C -target wasm-gc
            moonc build-package ./B/lib.mbt -o ./target/wasm-gc/release/bundle/B/B.core -pkg moonbitlang/core/B -i ./target/wasm-gc/release/bundle/A/A.mi:A -pkg-sources moonbitlang/core/B:./B -target wasm-gc
            moonc bundle-core ./target/wasm-gc/release/bundle/A/A.core ./target/wasm-gc/release/bundle/B/B.core ./target/wasm-gc/release/bundle/C/C.core ./target/wasm-gc/release/bundle/Orphan/Orphan.core ./target/wasm-gc/release/bundle/prelude/prelude.core -o ./target/wasm-gc/release/bundle/core.core
        "#]],
    );
}
