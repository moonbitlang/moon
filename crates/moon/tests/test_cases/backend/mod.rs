use super::*;

#[test]
#[ignore = "not implemented"]
fn test_backend_flag() {
    let dir = TestDir::new("backend/flag");

    check(
        get_stdout(&dir, ["check", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./lib/hello.mbt -o ./target/js/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib
            moonc check ./main/main.mbt -o ./target/js/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -i ./target/js/release/check/lib/lib.mi:lib -pkg-sources username/hello/main:./main
            moonc check ./lib/hello.mbt ./lib/hello_test.mbt -o ./target/js/release/check/lib/lib.underscore_test.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib
        "#]],
    );

    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/js/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib
            moonc build-package ./main/main.mbt -o ./target/js/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -i ./target/js/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/core.core ./target/js/release/build/lib/lib.core ./target/js/release/build/main/main.core -main username/hello/main -o ./target/js/release/build/main/main.js -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target js
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target/js/debug/test --mode test
            moonc build-package ./lib/hello.mbt ./lib/hello_test.mbt ./target/js/debug/test/lib/__generated_driver_for_underscore_test.mbt -o ./target/js/debug/test/lib/lib.underscore_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib -g -ryu
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/core.core ./target/js/debug/test/lib/lib.underscore_test.core -main username/hello/lib -o ./target/js/debug/test/lib/lib.underscore_test.js -test-mode -pkg-sources username/hello/lib:./lib -target js -ryu
            moonc build-package ./lib/hello.mbt ./target/js/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib -g -ryu
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/core.core ./target/js/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/js/debug/test/lib/lib.internal_test.js -test-mode -pkg-sources username/hello/lib:./lib -target js -ryu
        "#]],
    );

    check(
        get_stdout(&dir, ["bundle", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/js/release/bundle/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib
            moonc build-package ./main/main.mbt -o ./target/js/release/bundle/main/main.core -pkg username/hello/main -is-main -i ./target/js/release/bundle/lib/lib.mi:lib -pkg-sources username/hello/main:./main
            moonc bundle-core ./target/js/release/bundle/lib/lib.core ./target/js/release/bundle/main/main.core -o ./target/js/release/bundle/hello.core
        "#]],
    );
}

#[test]
fn test_js_format() {
    let dir = TestDir::new("backend/js_format");
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "js",
                "--sort-input",
                "--dry-run",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib3/hello.mbt -o ./target/js/debug/build/lib3/lib3.core -pkg username/hello/lib3 -pkg-sources username/hello/lib3:./lib3 -target js -g -O0 -source-map -workspace-path .
            moonc link-core ./target/js/debug/build/lib3/lib3.core -main username/hello/lib3 -o ./target/js/debug/build/lib3/lib3.js -pkg-config-path ./lib3/moon.pkg.json -pkg-sources username/hello/lib3:./lib3 -target js -g -O0 -source-map -exported_functions=hello -js-format iife
            moonc build-package ./lib2/hello.mbt -o ./target/js/debug/build/lib2/lib2.core -pkg username/hello/lib2 -pkg-sources username/hello/lib2:./lib2 -target js -g -O0 -source-map -workspace-path .
            moonc link-core ./target/js/debug/build/lib2/lib2.core -main username/hello/lib2 -o ./target/js/debug/build/lib2/lib2.js -pkg-config-path ./lib2/moon.pkg.json -pkg-sources username/hello/lib2:./lib2 -target js -g -O0 -source-map -exported_functions=hello -js-format cjs
            moonc build-package ./lib1/hello.mbt -o ./target/js/debug/build/lib1/lib1.core -pkg username/hello/lib1 -pkg-sources username/hello/lib1:./lib1 -target js -g -O0 -source-map -workspace-path .
            moonc link-core ./target/js/debug/build/lib1/lib1.core -main username/hello/lib1 -o ./target/js/debug/build/lib1/lib1.js -pkg-config-path ./lib1/moon.pkg.json -pkg-sources username/hello/lib1:./lib1 -target js -g -O0 -source-map -exported_functions=hello -js-format esm
            moonc build-package ./lib0/hello.mbt -o ./target/js/debug/build/lib0/lib0.core -pkg username/hello/lib0 -pkg-sources username/hello/lib0:./lib0 -target js -g -O0 -source-map -workspace-path .
            moonc link-core ./target/js/debug/build/lib0/lib0.core -main username/hello/lib0 -o ./target/js/debug/build/lib0/lib0.js -pkg-config-path ./lib0/moon.pkg.json -pkg-sources username/hello/lib0:./lib0 -target js -g -O0 -source-map -exported_functions=hello -js-format esm
        "#]],
    );
    let _ = get_stdout(&dir, ["build", "--target", "js", "--nostd"]);
    let t = dir.join("target").join("js").join("debug").join("build");
    check(
        std::fs::read_to_string(t.join("lib0").join("lib0.js"))
            .unwrap()
            .replace_crlf_to_lf(),
        expect![[r#"
            function username$hello$lib0$$hello() {
              return "Hello, world!";
            }
            export { username$hello$lib0$$hello as hello }
            //# sourceMappingURL=lib0.js.map
        "#]],
    );
    check(
        std::fs::read_to_string(t.join("lib1").join("lib1.js"))
            .unwrap()
            .replace_crlf_to_lf(),
        expect![[r#"
            function username$hello$lib1$$hello() {
              return "Hello, world!";
            }
            export { username$hello$lib1$$hello as hello }
            //# sourceMappingURL=lib1.js.map
        "#]],
    );
    check(
        std::fs::read_to_string(t.join("lib2").join("lib2.js"))
            .unwrap()
            .replace_crlf_to_lf(),
        expect![[r#"
            function username$hello$lib2$$hello() {
              return "Hello, world!";
            }
            exports.hello = username$hello$lib2$$hello;
            //# sourceMappingURL=lib2.js.map
        "#]],
    );
    check(
        std::fs::read_to_string(t.join("lib3").join("lib3.js"))
            .unwrap()
            .replace_crlf_to_lf(),
        expect![[r#"
            (() => {
              function username$hello$lib3$$hello() {
                return "Hello, world!";
              }
              globalThis.hello = username$hello$lib3$$hello;
            })();
            //# sourceMappingURL=lib3.js.map
        "#]],
    );
}
