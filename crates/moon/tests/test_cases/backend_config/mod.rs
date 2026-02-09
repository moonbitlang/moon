use super::*;

#[test]
fn test_backend_config() {
    let dir = TestDir::new("backend_config");

    let _ = get_stdout(&dir, ["build", "--output-wat"]);
    let out = std::fs::read_to_string(dir.join(format!(
        "_build/{}/debug/build/lib/lib.wat",
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
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main username/hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/build/lib/lib.core -main username/hello/lib -o ./_build/wasm-gc/debug/build/lib/lib.wasm -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map '-exported_functions=hello:hello_wasm_gc'
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
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg username/hello/main -is-main -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main username/hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map
            moonc link-core ./_build/wasm-gc/debug/build/lib/lib.core -main username/hello/lib -o ./_build/wasm-gc/debug/build/lib/lib.wasm -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map '-exported_functions=hello:hello_wasm_gc'
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
            moonc build-package ./lib/hello.mbt -o ./_build/js/debug/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target js -g -O0 -source-map -workspace-path . -all-pkgs ./_build/js/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/js/debug/build/main/main.core -pkg username/hello/main -is-main -i ./_build/js/debug/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target js -g -O0 -source-map -workspace-path . -all-pkgs ./_build/js/debug/build/all_pkgs.json
            moonc link-core ./_build/js/debug/build/lib/lib.core ./_build/js/debug/build/main/main.core -main username/hello/main -o ./_build/js/debug/build/main/main.js -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target js -g -O0 -source-map
            moonc link-core ./_build/js/debug/build/lib/lib.core -main username/hello/lib -o ./_build/js/debug/build/lib/lib.js -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -js-format esm -target js -g -O0 -source-map '-exported_functions=hello:hello_js'
        "#]],
    );
}
