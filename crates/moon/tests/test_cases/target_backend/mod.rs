use super::*;

#[test]
fn test_target_backend() {
    let dir = TestDir::new("target_backend");
    check(
        get_stdout(&dir, ["build", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./_build/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/release/build/lib/lib.core ./_build/wasm-gc/release/build/main/main.core -main hello/main -o ./_build/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--target", "wasm-gc", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./_build/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/release/build/lib/lib.core ./_build/wasm-gc/release/build/main/main.core -main hello/main -o ./_build/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--dry-run", "--target", "js", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/js/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target js -workspace-path . -all-pkgs ./_build/js/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/js/release/build/main/main.core -pkg hello/main -is-main -i ./_build/js/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target js -workspace-path . -all-pkgs ./_build/js/release/build/all_pkgs.json
            moonc link-core ./_build/js/release/build/lib/lib.core ./_build/js/release/build/main/main.core -main hello/main -o ./_build/js/release/build/main/main.js -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target js
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./_build/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/release/build/lib/lib.core ./_build/wasm-gc/release/build/main/main.core -main hello/main -o ./_build/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
        "#]],
    );
    check(
        get_stdout(&dir, ["run", "main", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./_build/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/release/build/lib/lib.core ./_build/wasm-gc/release/build/main/main.core -main hello/main -o ./_build/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
            moonrun ./_build/wasm-gc/release/build/main/main.wasm --
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["run", "main", "--dry-run", "--target", "wasm-gc", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./_build/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/release/build/lib/lib.core ./_build/wasm-gc/release/build/main/main.core -main hello/main -o ./_build/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
            moonrun ./_build/wasm-gc/release/build/main/main.wasm --
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["run", "main", "--dry-run", "--target", "js", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/js/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target js -workspace-path . -all-pkgs ./_build/js/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/js/release/build/main/main.core -pkg hello/main -is-main -i ./_build/js/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target js -workspace-path . -all-pkgs ./_build/js/release/build/all_pkgs.json
            moonc link-core ./_build/js/release/build/lib/lib.core ./_build/js/release/build/main/main.core -main hello/main -o ./_build/js/release/build/main/main.js -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target js
            node ./_build/js/release/build/main/main.js
        "#]],
    );
}
