use super::*;

#[test]
fn test_many_targets() {
    let dir = TestDir::new("targets/many_targets");
    check(
        get_stdout(&dir, ["test", "--target", "all", "--serial"]),
        expect![[r#"
            Total tests: 2, passed: 2, failed: 0. [wasm]
            Total tests: 2, passed: 2, failed: 0. [wasm-gc]
            Total tests: 2, passed: 2, failed: 0. [js]
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--target", "js,wasm", "--serial"]),
        expect![[r#"
            Total tests: 2, passed: 2, failed: 0. [wasm]
            Total tests: 2, passed: 2, failed: 0. [js]
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "check",
                "--target",
                "js,wasm",
                "--dry-run",
                "--serial",
                "--nostd",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./link/hello.mbt -o ./target/wasm/release/check/link/link.mi -pkg username/hello/link -pkg-sources username/hello/link:./link -target wasm
            moonc check ./lib/hello.mbt -o ./target/wasm/release/check/lib/lib.mi -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm
            moonc check ./link/hello.mbt -o ./target/js/release/check/link/link.mi -pkg username/hello/link -pkg-sources username/hello/link:./link -target js
            moonc check ./lib/hello.mbt -o ./target/js/release/check/lib/lib.mi -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target js
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "js,wasm",
                "--dry-run",
                "--serial",
                "--nostd",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./link/hello.mbt -o ./target/wasm/release/build/link/link.core -pkg username/hello/link -pkg-sources username/hello/link:./link -target wasm
            moonc link-core ./target/wasm/release/build/link/link.core -main username/hello/link -o ./target/wasm/release/build/link/link.wasm -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -target wasm
            moonc build-package ./link/hello.mbt -o ./target/js/release/build/link/link.core -pkg username/hello/link -pkg-sources username/hello/link:./link -target js
            moonc link-core ./target/js/release/build/link/link.core -main username/hello/link -o ./target/js/release/build/link/link.js -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -target js
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "bundle",
                "--target",
                "js,wasm",
                "--dry-run",
                "--serial",
                "--nostd",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm/release/bundle/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm
            moonc build-package ./link/hello.mbt -o ./target/wasm/release/bundle/link/link.core -pkg username/hello/link -pkg-sources username/hello/link:./link -target wasm
            moonc bundle-core ./target/wasm/release/bundle/lib/lib.core ./target/wasm/release/bundle/link/link.core -o ./target/wasm/release/bundle/hello.core
            moonc build-package ./lib/hello.mbt -o ./target/js/release/bundle/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target js
            moonc build-package ./link/hello.mbt -o ./target/js/release/bundle/link/link.core -pkg username/hello/link -pkg-sources username/hello/link:./link -target js
            moonc bundle-core ./target/js/release/bundle/lib/lib.core ./target/js/release/bundle/link/link.core -o ./target/js/release/bundle/hello.core
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "js,wasm",
                "--dry-run",
                "--serial",
                "--nostd",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moon generate-test-driver --output-driver ./target/wasm/debug/test/link/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm/debug/test/link/__blackbox_test_info.json --doctest-only ./link/hello.mbt --target wasm --pkg-name username/hello/link --driver-kind blackbox
            moonc build-package ./link/hello.mbt -o ./target/wasm/debug/test/link/link.core -pkg username/hello/link -pkg-sources username/hello/link:./link -target wasm -g -O0
            moonc build-package ./target/wasm/debug/test/link/__generated_driver_for_blackbox_test.mbt -doctest-only ./link/hello.mbt -o ./target/wasm/debug/test/link/link.blackbox_test.core -pkg username/hello/link_blackbox_test -is-main -i ./target/wasm/debug/test/link/link.mi:link -pkg-sources username/hello/link_blackbox_test:./link -target wasm -g -O0 -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/wasm/debug/test/link/link.core ./target/wasm/debug/test/link/link.blackbox_test.core -main username/hello/link_blackbox_test -o ./target/wasm/debug/test/link/link.blackbox_test.wasm -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -pkg-sources username/hello/link_blackbox_test:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --output-driver ./target/wasm/debug/test/link/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm/debug/test/link/__internal_test_info.json ./link/hello.mbt --target wasm --pkg-name username/hello/link --driver-kind internal
            moonc build-package ./link/hello.mbt ./target/wasm/debug/test/link/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/link/link.internal_test.core -pkg username/hello/link -is-main -pkg-sources username/hello/link:./link -target wasm -g -O0 -no-mi -test-mode
            moonc link-core ./target/wasm/debug/test/link/link.internal_test.core -main username/hello/link -o ./target/wasm/debug/test/link/link.internal_test.wasm -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --output-driver ./target/wasm/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm --pkg-name username/hello/lib --driver-kind blackbox
            moonc build-package ./lib/hello.mbt -o ./target/wasm/debug/test/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm -g -O0
            moonc build-package ./target/wasm/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -i ./target/wasm/debug/test/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm -g -O0 -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/wasm/debug/test/lib/lib.core ./target/wasm/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./target/wasm/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib_blackbox_test:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --output-driver ./target/wasm/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm --pkg-name username/hello/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target wasm -g -O0 -no-mi -test-mode
            moonc link-core ./target/wasm/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --output-driver ./target/js/debug/test/link/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/js/debug/test/link/__blackbox_test_info.json --doctest-only ./link/hello.mbt --target js --pkg-name username/hello/link --driver-kind blackbox
            moonc build-package ./link/hello.mbt -o ./target/js/debug/test/link/link.core -pkg username/hello/link -pkg-sources username/hello/link:./link -target js -g -O0 -source-map
            moonc build-package ./target/js/debug/test/link/__generated_driver_for_blackbox_test.mbt -doctest-only ./link/hello.mbt -o ./target/js/debug/test/link/link.blackbox_test.core -pkg username/hello/link_blackbox_test -is-main -i ./target/js/debug/test/link/link.mi:link -pkg-sources username/hello/link_blackbox_test:./link -target js -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/js/debug/test/link/link.core ./target/js/debug/test/link/link.blackbox_test.core -main username/hello/link_blackbox_test -o ./target/js/debug/test/link/link.blackbox_test.js -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -pkg-sources username/hello/link_blackbox_test:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/js/debug/test/link/__generated_driver_for_internal_test.mbt --output-metadata ./target/js/debug/test/link/__internal_test_info.json ./link/hello.mbt --target js --pkg-name username/hello/link --driver-kind internal
            moonc build-package ./link/hello.mbt ./target/js/debug/test/link/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/link/link.internal_test.core -pkg username/hello/link -is-main -pkg-sources username/hello/link:./link -target js -g -O0 -source-map -no-mi -test-mode
            moonc link-core ./target/js/debug/test/link/link.internal_test.core -main username/hello/link -o ./target/js/debug/test/link/link.internal_test.js -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/js/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/js/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target js --pkg-name username/hello/lib --driver-kind blackbox
            moonc build-package ./lib/hello.mbt -o ./target/js/debug/test/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target js -g -O0 -source-map
            moonc build-package ./target/js/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/js/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -i ./target/js/debug/test/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./lib -target js -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/js/debug/test/lib/lib.core ./target/js/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./target/js/debug/test/lib/lib.blackbox_test.js -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib_blackbox_test:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/js/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/js/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target js --pkg-name username/hello/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/js/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target js -g -O0 -source-map -no-mi -test-mode
            moonc link-core ./target/js/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/js/debug/test/lib/lib.internal_test.js -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "js,wasm",
                "--dry-run",
                "--serial",
                "--nostd",
                "--sort-input",
                "-p",
                "username/hello/lib",
                "-f",
                "hello.mbt",
                "-i",
                "0",
            ],
        ),
        expect![[r#"
            moon generate-test-driver --output-driver ./target/wasm/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm --pkg-name username/hello/lib --driver-kind blackbox
            moonc build-package ./lib/hello.mbt -o ./target/wasm/debug/test/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm -g -O0
            moonc build-package ./target/wasm/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -i ./target/wasm/debug/test/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm -g -O0 -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/wasm/debug/test/lib/lib.core ./target/wasm/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./target/wasm/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib_blackbox_test:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --output-driver ./target/wasm/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm --pkg-name username/hello/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target wasm -g -O0 -no-mi -test-mode
            moonc link-core ./target/wasm/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --output-driver ./target/js/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/js/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target js --pkg-name username/hello/lib --driver-kind blackbox
            moonc build-package ./lib/hello.mbt -o ./target/js/debug/test/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target js -g -O0 -source-map
            moonc build-package ./target/js/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/js/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -i ./target/js/debug/test/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./lib -target js -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/js/debug/test/lib/lib.core ./target/js/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./target/js/debug/test/lib/lib.blackbox_test.js -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib_blackbox_test:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/js/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/js/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target js --pkg-name username/hello/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/js/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target js -g -O0 -source-map -no-mi -test-mode
            moonc link-core ./target/js/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/js/debug/test/lib/lib.internal_test.js -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "js,wasm,all",
                "--dry-run",
                "--serial",
                "--nostd",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moon generate-test-driver --output-driver ./target/wasm/debug/test/link/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm/debug/test/link/__blackbox_test_info.json --doctest-only ./link/hello.mbt --target wasm --pkg-name username/hello/link --driver-kind blackbox
            moonc build-package ./link/hello.mbt -o ./target/wasm/debug/test/link/link.core -pkg username/hello/link -pkg-sources username/hello/link:./link -target wasm -g -O0
            moonc build-package ./target/wasm/debug/test/link/__generated_driver_for_blackbox_test.mbt -doctest-only ./link/hello.mbt -o ./target/wasm/debug/test/link/link.blackbox_test.core -pkg username/hello/link_blackbox_test -is-main -i ./target/wasm/debug/test/link/link.mi:link -pkg-sources username/hello/link_blackbox_test:./link -target wasm -g -O0 -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/wasm/debug/test/link/link.core ./target/wasm/debug/test/link/link.blackbox_test.core -main username/hello/link_blackbox_test -o ./target/wasm/debug/test/link/link.blackbox_test.wasm -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -pkg-sources username/hello/link_blackbox_test:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --output-driver ./target/wasm/debug/test/link/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm/debug/test/link/__internal_test_info.json ./link/hello.mbt --target wasm --pkg-name username/hello/link --driver-kind internal
            moonc build-package ./link/hello.mbt ./target/wasm/debug/test/link/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/link/link.internal_test.core -pkg username/hello/link -is-main -pkg-sources username/hello/link:./link -target wasm -g -O0 -no-mi -test-mode
            moonc link-core ./target/wasm/debug/test/link/link.internal_test.core -main username/hello/link -o ./target/wasm/debug/test/link/link.internal_test.wasm -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --output-driver ./target/wasm/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm --pkg-name username/hello/lib --driver-kind blackbox
            moonc build-package ./lib/hello.mbt -o ./target/wasm/debug/test/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm -g -O0
            moonc build-package ./target/wasm/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -i ./target/wasm/debug/test/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm -g -O0 -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/wasm/debug/test/lib/lib.core ./target/wasm/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./target/wasm/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib_blackbox_test:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --output-driver ./target/wasm/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm --pkg-name username/hello/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target wasm -g -O0 -no-mi -test-mode
            moonc link-core ./target/wasm/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/link/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/link/__blackbox_test_info.json --doctest-only ./link/hello.mbt --target wasm-gc --pkg-name username/hello/link --driver-kind blackbox
            moonc build-package ./link/hello.mbt -o ./target/wasm-gc/debug/test/link/link.core -pkg username/hello/link -pkg-sources username/hello/link:./link -target wasm-gc -g -O0 -source-map
            moonc build-package ./target/wasm-gc/debug/test/link/__generated_driver_for_blackbox_test.mbt -doctest-only ./link/hello.mbt -o ./target/wasm-gc/debug/test/link/link.blackbox_test.core -pkg username/hello/link_blackbox_test -is-main -i ./target/wasm-gc/debug/test/link/link.mi:link -pkg-sources username/hello/link_blackbox_test:./link -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/wasm-gc/debug/test/link/link.core ./target/wasm-gc/debug/test/link/link.blackbox_test.core -main username/hello/link_blackbox_test -o ./target/wasm-gc/debug/test/link/link.blackbox_test.wasm -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -pkg-sources username/hello/link_blackbox_test:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/link/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/link/__internal_test_info.json ./link/hello.mbt --target wasm-gc --pkg-name username/hello/link --driver-kind internal
            moonc build-package ./link/hello.mbt ./target/wasm-gc/debug/test/link/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/link/link.internal_test.core -pkg username/hello/link -is-main -pkg-sources username/hello/link:./link -target wasm-gc -g -O0 -source-map -no-mi -test-mode
            moonc link-core ./target/wasm-gc/debug/test/link/link.internal_test.core -main username/hello/link -o ./target/wasm-gc/debug/test/link/link.internal_test.wasm -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --driver-kind blackbox
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map
            moonc build-package ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib_blackbox_test:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -no-mi -test-mode
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/js/debug/test/link/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/js/debug/test/link/__blackbox_test_info.json --doctest-only ./link/hello.mbt --target js --pkg-name username/hello/link --driver-kind blackbox
            moonc build-package ./link/hello.mbt -o ./target/js/debug/test/link/link.core -pkg username/hello/link -pkg-sources username/hello/link:./link -target js -g -O0 -source-map
            moonc build-package ./target/js/debug/test/link/__generated_driver_for_blackbox_test.mbt -doctest-only ./link/hello.mbt -o ./target/js/debug/test/link/link.blackbox_test.core -pkg username/hello/link_blackbox_test -is-main -i ./target/js/debug/test/link/link.mi:link -pkg-sources username/hello/link_blackbox_test:./link -target js -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/js/debug/test/link/link.core ./target/js/debug/test/link/link.blackbox_test.core -main username/hello/link_blackbox_test -o ./target/js/debug/test/link/link.blackbox_test.js -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -pkg-sources username/hello/link_blackbox_test:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/js/debug/test/link/__generated_driver_for_internal_test.mbt --output-metadata ./target/js/debug/test/link/__internal_test_info.json ./link/hello.mbt --target js --pkg-name username/hello/link --driver-kind internal
            moonc build-package ./link/hello.mbt ./target/js/debug/test/link/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/link/link.internal_test.core -pkg username/hello/link -is-main -pkg-sources username/hello/link:./link -target js -g -O0 -source-map -no-mi -test-mode
            moonc link-core ./target/js/debug/test/link/link.internal_test.core -main username/hello/link -o ./target/js/debug/test/link/link.internal_test.js -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/js/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/js/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target js --pkg-name username/hello/lib --driver-kind blackbox
            moonc build-package ./lib/hello.mbt -o ./target/js/debug/test/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target js -g -O0 -source-map
            moonc build-package ./target/js/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/js/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -i ./target/js/debug/test/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./lib -target js -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/js/debug/test/lib/lib.core ./target/js/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./target/js/debug/test/lib/lib.blackbox_test.js -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib_blackbox_test:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/js/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/js/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target js --pkg-name username/hello/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/js/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target js -g -O0 -source-map -no-mi -test-mode
            moonc link-core ./target/js/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/js/debug/test/lib/lib.internal_test.js -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "all",
                "--dry-run",
                "--serial",
                "--nostd",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moon generate-test-driver --output-driver ./target/wasm/debug/test/link/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm/debug/test/link/__blackbox_test_info.json --doctest-only ./link/hello.mbt --target wasm --pkg-name username/hello/link --driver-kind blackbox
            moonc build-package ./link/hello.mbt -o ./target/wasm/debug/test/link/link.core -pkg username/hello/link -pkg-sources username/hello/link:./link -target wasm -g -O0
            moonc build-package ./target/wasm/debug/test/link/__generated_driver_for_blackbox_test.mbt -doctest-only ./link/hello.mbt -o ./target/wasm/debug/test/link/link.blackbox_test.core -pkg username/hello/link_blackbox_test -is-main -i ./target/wasm/debug/test/link/link.mi:link -pkg-sources username/hello/link_blackbox_test:./link -target wasm -g -O0 -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/wasm/debug/test/link/link.core ./target/wasm/debug/test/link/link.blackbox_test.core -main username/hello/link_blackbox_test -o ./target/wasm/debug/test/link/link.blackbox_test.wasm -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -pkg-sources username/hello/link_blackbox_test:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --output-driver ./target/wasm/debug/test/link/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm/debug/test/link/__internal_test_info.json ./link/hello.mbt --target wasm --pkg-name username/hello/link --driver-kind internal
            moonc build-package ./link/hello.mbt ./target/wasm/debug/test/link/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/link/link.internal_test.core -pkg username/hello/link -is-main -pkg-sources username/hello/link:./link -target wasm -g -O0 -no-mi -test-mode
            moonc link-core ./target/wasm/debug/test/link/link.internal_test.core -main username/hello/link -o ./target/wasm/debug/test/link/link.internal_test.wasm -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --output-driver ./target/wasm/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm --pkg-name username/hello/lib --driver-kind blackbox
            moonc build-package ./lib/hello.mbt -o ./target/wasm/debug/test/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm -g -O0
            moonc build-package ./target/wasm/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -i ./target/wasm/debug/test/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm -g -O0 -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/wasm/debug/test/lib/lib.core ./target/wasm/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./target/wasm/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib_blackbox_test:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --output-driver ./target/wasm/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm --pkg-name username/hello/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target wasm -g -O0 -no-mi -test-mode
            moonc link-core ./target/wasm/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/link/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/link/__blackbox_test_info.json --doctest-only ./link/hello.mbt --target wasm-gc --pkg-name username/hello/link --driver-kind blackbox
            moonc build-package ./link/hello.mbt -o ./target/wasm-gc/debug/test/link/link.core -pkg username/hello/link -pkg-sources username/hello/link:./link -target wasm-gc -g -O0 -source-map
            moonc build-package ./target/wasm-gc/debug/test/link/__generated_driver_for_blackbox_test.mbt -doctest-only ./link/hello.mbt -o ./target/wasm-gc/debug/test/link/link.blackbox_test.core -pkg username/hello/link_blackbox_test -is-main -i ./target/wasm-gc/debug/test/link/link.mi:link -pkg-sources username/hello/link_blackbox_test:./link -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/wasm-gc/debug/test/link/link.core ./target/wasm-gc/debug/test/link/link.blackbox_test.core -main username/hello/link_blackbox_test -o ./target/wasm-gc/debug/test/link/link.blackbox_test.wasm -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -pkg-sources username/hello/link_blackbox_test:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/link/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/link/__internal_test_info.json ./link/hello.mbt --target wasm-gc --pkg-name username/hello/link --driver-kind internal
            moonc build-package ./link/hello.mbt ./target/wasm-gc/debug/test/link/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/link/link.internal_test.core -pkg username/hello/link -is-main -pkg-sources username/hello/link:./link -target wasm-gc -g -O0 -source-map -no-mi -test-mode
            moonc link-core ./target/wasm-gc/debug/test/link/link.internal_test.core -main username/hello/link -o ./target/wasm-gc/debug/test/link/link.internal_test.wasm -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --driver-kind blackbox
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map
            moonc build-package ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib_blackbox_test:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -no-mi -test-mode
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/js/debug/test/link/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/js/debug/test/link/__blackbox_test_info.json --doctest-only ./link/hello.mbt --target js --pkg-name username/hello/link --driver-kind blackbox
            moonc build-package ./link/hello.mbt -o ./target/js/debug/test/link/link.core -pkg username/hello/link -pkg-sources username/hello/link:./link -target js -g -O0 -source-map
            moonc build-package ./target/js/debug/test/link/__generated_driver_for_blackbox_test.mbt -doctest-only ./link/hello.mbt -o ./target/js/debug/test/link/link.blackbox_test.core -pkg username/hello/link_blackbox_test -is-main -i ./target/js/debug/test/link/link.mi:link -pkg-sources username/hello/link_blackbox_test:./link -target js -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/js/debug/test/link/link.core ./target/js/debug/test/link/link.blackbox_test.core -main username/hello/link_blackbox_test -o ./target/js/debug/test/link/link.blackbox_test.js -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -pkg-sources username/hello/link_blackbox_test:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/js/debug/test/link/__generated_driver_for_internal_test.mbt --output-metadata ./target/js/debug/test/link/__internal_test_info.json ./link/hello.mbt --target js --pkg-name username/hello/link --driver-kind internal
            moonc build-package ./link/hello.mbt ./target/js/debug/test/link/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/link/link.internal_test.core -pkg username/hello/link -is-main -pkg-sources username/hello/link:./link -target js -g -O0 -source-map -no-mi -test-mode
            moonc link-core ./target/js/debug/test/link/link.internal_test.core -main username/hello/link -o ./target/js/debug/test/link/link.internal_test.js -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/js/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/js/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target js --pkg-name username/hello/lib --driver-kind blackbox
            moonc build-package ./lib/hello.mbt -o ./target/js/debug/test/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target js -g -O0 -source-map
            moonc build-package ./target/js/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/js/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -i ./target/js/debug/test/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./lib -target js -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/js/debug/test/lib/lib.core ./target/js/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./target/js/debug/test/lib/lib.blackbox_test.js -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib_blackbox_test:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/js/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/js/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target js --pkg-name username/hello/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/js/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target js -g -O0 -source-map -no-mi -test-mode
            moonc link-core ./target/js/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/js/debug/test/lib/lib.internal_test.js -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0 -source-map
        "#]],
    );
}

#[test]
fn test_many_targets_auto_update_001() {
    let dir = TestDir::new("targets/auto_update");
    let _ = get_stdout(&dir, ["test", "-u", "--no-parallelize"]);
    check(
        read(dir.join("lib").join("x.wasm.mbt")),
        expect![[r#"
            test {
              inspect("wasm")
            }
        "#]],
    );
    check(
        read(dir.join("lib").join("x.wasm-gc.mbt")),
        expect![[r#"
            test {
              inspect("wasm-gc", content="wasm-gc")
            }
        "#]],
    );
    check(
        read(dir.join("lib").join("x.js.mbt")),
        expect![[r#"
            test {
              inspect("js")
            }
        "#]],
    );

    check(
        read(dir.join("lib").join("x.native.mbt")),
        expect![[r#"
                test {
                  inspect("native")
                }
            "#]],
    );
}

#[test]
fn test_many_targets_auto_update_002() {
    let dir = TestDir::new("targets/auto_update");
    let _ = get_stdout(&dir, ["test", "--target", "js", "-u", "--no-parallelize"]);
    check(
        read(dir.join("lib").join("x.wasm.mbt")),
        expect![[r#"
            test {
              inspect("wasm")
            }
        "#]],
    );
    check(
        read(dir.join("lib").join("x.wasm-gc.mbt")),
        expect![[r#"
            test {
              inspect("wasm-gc")
            }
        "#]],
    );
    check(
        read(dir.join("lib").join("x.js.mbt")),
        expect![[r#"
            test {
              inspect("js", content="js")
            }
        "#]],
    );

    check(
        read(dir.join("lib").join("x.native.mbt")),
        expect![[r#"
            test {
              inspect("native")
            }
            "#]],
    );

    let _ = get_stdout(
        &dir,
        ["test", "--target", "native", "-u", "--no-parallelize"],
    );
    check(
        read(dir.join("lib").join("x.native.mbt")),
        expect![[r#"
            test {
              inspect("native", content="native")
            }
        "#]],
    );
}

#[test]
fn test_many_targets_auto_update_003() {
    let dir = TestDir::new("targets/auto_update");
    let _ = get_stdout(&dir, ["test", "--target", "wasm", "-u", "--no-parallelize"]);
    check(
        read(dir.join("lib").join("x.wasm.mbt")),
        expect![[r#"
            test {
              inspect("wasm", content="wasm")
            }
        "#]],
    );
    check(
        read(dir.join("lib").join("x.wasm-gc.mbt")),
        expect![[r#"
            test {
              inspect("wasm-gc")
            }
        "#]],
    );
    let _ = get_stdout(&dir, ["test", "--target", "js", "-u", "--no-parallelize"]);
    check(
        read(dir.join("lib").join("x.js.mbt")),
        expect![[r#"
            test {
              inspect("js", content="js")
            }
        "#]],
    );
}

#[test]
fn test_many_targets_auto_update_004() {
    let dir = TestDir::new("targets/auto_update");
    let _ = get_stdout(&dir, ["test", "--target", "wasm", "-u", "--no-parallelize"]);
    check(
        read(dir.join("lib").join("x.wasm.mbt")),
        expect![[r#"
            test {
              inspect("wasm", content="wasm")
            }
        "#]],
    );
    let _ = get_stdout(
        &dir,
        ["test", "--target", "wasm-gc", "-u", "--no-parallelize"],
    );
    check(
        read(dir.join("lib").join("x.wasm-gc.mbt")),
        expect![[r#"
            test {
              inspect("wasm-gc", content="wasm-gc")
            }
        "#]],
    );
    let _ = get_stdout(&dir, ["test", "--target", "js", "-u", "--no-parallelize"]);
    check(
        read(dir.join("lib").join("x.js.mbt")),
        expect![[r#"
            test {
              inspect("js", content="js")
            }
        "#]],
    );
}

#[test]
fn test_many_targets_expect_failed() {
    let dir = TestDir::new("targets/expect_failed");
    check(
        get_err_stdout(
            &dir,
            ["test", "--target", "all", "--serial", "--sort-input"],
        ),
        expect![[r#"
            test username/hello/lib/x.wasm.mbt::0 failed
            expect test failed at $ROOT/lib/x.wasm.mbt:2:3-2:31
            Diff:
            ----
            0wasm
            ----

            Total tests: 1, passed: 0, failed: 1. [wasm]
            test username/hello/lib/x.wasm-gc.mbt::0 failed
            expect test failed at $ROOT/lib/x.wasm-gc.mbt:2:3-2:34
            Diff:
            ----
            1wasm-gc
            ----

            Total tests: 1, passed: 0, failed: 1. [wasm-gc]
            test username/hello/lib/x.js.mbt::0 failed
            expect test failed at $ROOT/lib/x.js.mbt:2:3-2:29
            Diff:
            ----
            2js
            ----

            Total tests: 1, passed: 0, failed: 1. [js]
        "#]],
    );
    check(
        get_err_stdout(
            &dir,
            ["test", "--target", "js,wasm", "--sort-input", "--serial"],
        ),
        expect![[r#"
            test username/hello/lib/x.wasm.mbt::0 failed
            expect test failed at $ROOT/lib/x.wasm.mbt:2:3-2:31
            Diff:
            ----
            0wasm
            ----

            Total tests: 1, passed: 0, failed: 1. [wasm]
            test username/hello/lib/x.js.mbt::0 failed
            expect test failed at $ROOT/lib/x.js.mbt:2:3-2:29
            Diff:
            ----
            2js
            ----

            Total tests: 1, passed: 0, failed: 1. [js]
        "#]],
    );

    check(
        get_err_stdout(
            &dir,
            [
                "test",
                "--target",
                "js,wasm,native",
                "--sort-input",
                "--serial",
            ],
        ),
        expect![[r#"
            test username/hello/lib/x.wasm.mbt::0 failed
            expect test failed at $ROOT/lib/x.wasm.mbt:2:3-2:31
            Diff:
            ----
            0wasm
            ----

            Total tests: 1, passed: 0, failed: 1. [wasm]
            test username/hello/lib/x.js.mbt::0 failed
            expect test failed at $ROOT/lib/x.js.mbt:2:3-2:29
            Diff:
            ----
            2js
            ----

            Total tests: 1, passed: 0, failed: 1. [js]
            test username/hello/lib/x.native.mbt::0 failed
            expect test failed at $ROOT/lib/x.native.mbt:2:3-2:33
            Diff:
            ----
            3native
            ----

            Total tests: 1, passed: 0, failed: 1. [native]
        "#]],
    );
}
