use super::*;

#[test]
fn test_target_backend() {
    let dir = TestDir::new("target_backend");
    check(
        get_stdout(&dir, ["build", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--target", "wasm-gc", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--dry-run", "--target", "js", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/js/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target js -g -O0 -source-map -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/js/debug/build/main/main.core -pkg hello/main -is-main -i ./target/js/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target js -g -O0 -source-map -workspace-path .
            moonc link-core ./target/js/debug/build/lib/lib.core ./target/js/debug/build/main/main.core -main hello/main -o ./target/js/debug/build/main/main.js -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target js -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["run", "main", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
            moonrun ./target/wasm-gc/debug/build/main/main.wasm
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["run", "main", "--dry-run", "--target", "wasm-gc", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
            moonrun ./target/wasm-gc/debug/build/main/main.wasm
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["run", "main", "--dry-run", "--target", "js", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/js/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target js -g -O0 -source-map -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/js/debug/build/main/main.core -pkg hello/main -is-main -i ./target/js/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target js -g -O0 -source-map -workspace-path .
            moonc link-core ./target/js/debug/build/lib/lib.core ./target/js/debug/build/main/main.core -main hello/main -o ./target/js/debug/build/main/main.js -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target js -g -O0 -source-map
            node ./target/js/debug/build/main/main.js
        "#]],
    );
}
