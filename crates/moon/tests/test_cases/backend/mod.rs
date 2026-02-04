use super::*;
use moonutil::common::BUILD_DIR;

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
    let stdout = get_stdout(
        &dir,
        [
            "build",
            "--target",
            "js",
            "--sort-input",
            "--dry-run",
            "--nostd",
            "--release",
        ],
    );
    println!("{stdout}");
    let stdout_lines = stdout.lines().collect::<Vec<_>>();

    let get_link_core_of = |name: &str| {
        let output = format!("{name}.js");
        let found = stdout_lines
            .iter()
            .find(|line| line.contains(&output) && line.contains("moonc link-core"));
        found.unwrap_or_else(|| panic!("Expected to find link-core command for {name}"))
    };

    // lib0 -- default
    // lib1 -- esm
    // lib2 -- cjs
    // lib3 -- iife
    assert!(get_link_core_of("lib0").contains("-target js"));
    assert!(get_link_core_of("lib0").contains("-js-format esm"));
    assert!(get_link_core_of("lib1").contains("-target js"));
    assert!(get_link_core_of("lib1").contains("-js-format esm"));
    assert!(get_link_core_of("lib2").contains("-target js"));
    assert!(get_link_core_of("lib2").contains("-js-format cjs"));
    assert!(get_link_core_of("lib3").contains("-target js"));
    assert!(get_link_core_of("lib3").contains("-js-format iife"));

    let _ = get_stdout(&dir, ["build", "--target", "js", "--nostd", "--release"]);
    let t = dir.join(BUILD_DIR).join("js").join("release").join("build");
    check(
        std::fs::read_to_string(t.join("lib0").join("lib0.js"))
            .unwrap()
            .replace_crlf_to_lf(),
        expect![[r#"
            function _M0FP38username5hello4lib05hello() {
              return "Hello, world!";
            }
            export { _M0FP38username5hello4lib05hello as hello }
        "#]],
    );
    check(
        std::fs::read_to_string(t.join("lib1").join("lib1.js"))
            .unwrap()
            .replace_crlf_to_lf(),
        expect![[r#"
            function _M0FP38username5hello4lib15hello() {
              return "Hello, world!";
            }
            export { _M0FP38username5hello4lib15hello as hello }
        "#]],
    );
    check(
        std::fs::read_to_string(t.join("lib2").join("lib2.js"))
            .unwrap()
            .replace_crlf_to_lf(),
        expect![[r#"
            function _M0FP38username5hello4lib25hello() {
              return "Hello, world!";
            }
            exports.hello = _M0FP38username5hello4lib25hello;
        "#]],
    );
    check(
        std::fs::read_to_string(t.join("lib3").join("lib3.js"))
            .unwrap()
            .replace_crlf_to_lf(),
        expect![[r#"
            (() => {
              function _M0FP38username5hello4lib35hello() {
                return "Hello, world!";
              }
              globalThis.hello = _M0FP38username5hello4lib35hello;
            })();
        "#]],
    );
}
