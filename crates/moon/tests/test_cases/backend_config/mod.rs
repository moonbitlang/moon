use super::*;

#[test]
fn test_backend_config() {
    let dir = TestDir::new("backend_config");

    let _ = get_stdout(&dir, ["build", "--output-wat"]);
    let out = std::fs::read_to_string(dir.join(format!(
        "target/{}/release/build/lib/lib.wat",
        TargetBackend::default().to_backend_ext()
    )))
    .unwrap();
    assert!(out.contains(&format!(
        "export \"hello_{}\"",
        TargetBackend::default().to_backend_ext().replace('-', "_")
    )));
    assert_command_matches(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core -main username/hello/lib -o ./target/wasm-gc/release/build/lib/lib.wasm -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -exported_functions=hello:hello_wasm_gc
        "#]],
    );

    assert_command_matches(
        get_stdout(
            &dir,
            [
                "build",
                "--dry-run",
                "--nostd",
                "--target",
                "wasm-gc",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path .
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core -main username/hello/lib -o ./target/wasm-gc/release/build/lib/lib.wasm -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -target wasm-gc -exported_functions=hello:hello_wasm_gc
        "#]],
    );

    assert_command_matches(
        get_stdout(
            &dir,
            [
                "build",
                "--dry-run",
                "--nostd",
                "--target",
                "js",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/js/release/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target js -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/js/release/build/main/main.core -pkg username/hello/main -is-main -i ./target/js/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target js -workspace-path .
            moonc link-core ./target/js/release/build/lib/lib.core ./target/js/release/build/main/main.core -main username/hello/main -o ./target/js/release/build/main/main.js -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target js
            moonc link-core ./target/js/release/build/lib/lib.core -main username/hello/lib -o ./target/js/release/build/lib/lib.js -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -js-format esm -target js -exported_functions=hello:hello_js
        "#]],
    );
}
