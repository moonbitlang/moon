use super::*;

#[test]
fn test_moon_bundle() {
    let dir = TestDir::new("moon_bundle");
    check(
        get_stdout(&dir, ["bundle", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package -w -a -o ./_build/wasm-gc/release/bundle/prelude/prelude.core -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/bundle/all_pkgs.json
            moonc build-package ./Orphan/lib.mbt -w -a -o ./_build/wasm-gc/release/bundle/Orphan/Orphan.core -pkg moonbitlang/core/Orphan -pkg-sources moonbitlang/core/Orphan:./Orphan -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/bundle/all_pkgs.json
            moonc build-package ./A/lib.mbt -w -a -o ./_build/wasm-gc/release/bundle/A/A.core -pkg moonbitlang/core/A -pkg-sources moonbitlang/core/A:./A -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/bundle/all_pkgs.json
            moonc build-package ./C/lib.mbt -w -a -o ./_build/wasm-gc/release/bundle/C/C.core -pkg moonbitlang/core/C -i ./_build/wasm-gc/release/bundle/A/A.mi:A -pkg-sources moonbitlang/core/C:./C -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/bundle/all_pkgs.json
            moonc build-package ./B/lib.mbt -w -a -o ./_build/wasm-gc/release/bundle/B/B.core -pkg moonbitlang/core/B -i ./_build/wasm-gc/release/bundle/A/A.mi:A -pkg-sources moonbitlang/core/B:./B -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/bundle/all_pkgs.json
            moonc bundle-core ./_build/wasm-gc/release/bundle/A/A.core ./_build/wasm-gc/release/bundle/B/B.core ./_build/wasm-gc/release/bundle/C/C.core ./_build/wasm-gc/release/bundle/Orphan/Orphan.core ./_build/wasm-gc/release/bundle/prelude/prelude.core -o ./_build/wasm-gc/release/bundle/core.core
        "#]],
    );
}
