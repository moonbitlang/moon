mod skip_test;

use super::*;

#[test]
fn test_moon_test_filter_package() {
    let dir = TestDir::new("test_filter/test_filter");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            test C
            test D
            test A
            test B
            test hello_0
            test hello_1
            test hello_2
            Total tests: 7, passed: 7, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            test A
            test hello_0
            test hello_1
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_multi_package() {
    let dir = TestDir::new("test_filter/pkg_with_test_imports");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib",
                "username/hello/lib1",
                "username/hello/lib2",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib1

            Hello from lib2

            Hello from lib7
            Hello from lib3

            Hello from lib4
            Total tests: 4, passed: 4, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib",
                "username/hello/lib1",
                "username/hello/lib2",
                "-p",
                "username/hello/lib3",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib1

            Hello from lib2

            Hello from lib7
            Hello from lib3

            Hello from lib4
            Hello from lib3

            Hello from lib7
            Hello from lib6
            Total tests: 7, passed: 7, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib",
                "username/hello/lib1",
                "username/hello/lib2",
                "-f",
                "lib.mbt",
                "-p",
                "username/hello/lib3",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib1

            Hello from lib2

            Hello from lib3

            Hello from lib4
            Hello from lib3

            Hello from lib7
            Total tests: 5, passed: 5, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib",
                "username/hello/lib1",
                "username/hello/lib2",
                "-f",
                "lib.mbt",
                "-p",
                "username/hello/lib3",
                "-i",
                "0",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib1

            Hello from lib2

            Hello from lib3

            Hello from lib4
            Hello from lib3

            Total tests: 4, passed: 4, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_package_with_singlefile() {
    let dir = TestDir::new("test_filter/test_filter");

    check(
        get_stdout(&dir, ["test", "A/hello.mbt"]),
        expect![[r#"
            test A
            test B
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "A/hello_wbtest.mbt"]),
        expect![[r#"
            test hello_0
            test hello_1
            test hello_2
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );

    check(
        get_stdout(&dir.join("A"), ["test", "hello_wbtest.mbt"]),
        expect![[r#"
            test hello_0
            test hello_1
            test hello_2
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_package_with_folder() {
    let dir = TestDir::new("test_filter/test_filter");

    check(
        get_stdout(&dir, ["test", "A", "--sort-input", "--no-parallelize"]),
        expect![[r#"
            test C
            test D
            test A
            test B
            test hello_0
            test hello_1
            test hello_2
            Total tests: 7, passed: 7, failed: 0.
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "A/", "--sort-input", "--no-parallelize"]),
        expect![[r#"
            test C
            test D
            test A
            test B
            test hello_0
            test hello_1
            test hello_2
            Total tests: 7, passed: 7, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir.join("A"),
            ["test", ".", "--sort-input", "--no-parallelize"],
        ),
        expect![[r#"
            test C
            test D
            test A
            test B
            test hello_0
            test hello_1
            test hello_2
            Total tests: 7, passed: 7, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_package_with_deps() {
    let dir = TestDir::new("test_filter/pkg_with_deps");

    check(
        get_stdout(
            &dir,
            ["test", "-p", "username/hello/lib", "--no-parallelize"],
        ),
        expect![[r#"
            Hello from lib1
            Hello from lib2
            Hello from lib4

            Hello from lib3
            Hello from lib4


            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["test", "-p", "username/hello/lib2", "--no-parallelize"],
        ),
        expect![[r#"
            Hello from lib2
            Hello from lib4

            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["test", "-p", "username/hello/lib4", "--no-parallelize"],
        ),
        expect![[r#"
            Hello from lib4
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_package_with_test_imports() {
    let dir = TestDir::new("test_filter/pkg_with_test_imports");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib1

            Hello from lib2

            Hello from lib7
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib1",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib3

            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib2",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib4
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib3",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib3

            Hello from lib7
            Hello from lib6
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib4",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib5
            Hello from lib5
            Hello from lib7
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib5",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib5
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib6",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib6
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib7",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib7
            Hello from lib6
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_parallelism() {
    let dir = TestDir::new("test_filter/pkg_with_test_imports");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-j1",
                "-p",
                "username/hello/lib",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib1

            Hello from lib2

            Hello from lib7
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-j1",
                "-p",
                "username/hello/lib1",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib3

            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-j1",
                "-p",
                "username/hello/lib2",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib4
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-j1",
                "-p",
                "username/hello/lib3",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib3

            Hello from lib7
            Hello from lib6
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-j1",
                "-p",
                "username/hello/lib4",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib5
            Hello from lib5
            Hello from lib7
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-j1",
                "-p",
                "username/hello/lib5",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib5
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-j1",
                "-p",
                "username/hello/lib6",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib6
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-j1",
                "-p",
                "username/hello/lib7",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib7
            Hello from lib6
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_package_dry_run() {
    let dir = TestDir::new("test_filter/test_filter");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/A/__generated_driver_for_whitebox_test.mbt --output-metadata ./target/wasm-gc/debug/test/A/__whitebox_test_info.json ./A/hello_wbtest.mbt --target wasm-gc --pkg-name username/hello/A --driver-kind whitebox
            moonc build-package ./A/hello.mbt ./A/test.mbt ./A/hello_wbtest.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/A/A.whitebox_test.core -pkg username/hello/A -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -g -O0 -source-map -whitebox-test -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/A/A.whitebox_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.whitebox_test.wasm -test-mode -pkg-config-path ./A/moon.pkg.json -pkg-sources username/hello/A:./A -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/A/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/A/__internal_test_info.json ./A/hello.mbt ./A/test.mbt --target wasm-gc --pkg-name username/hello/A --driver-kind internal
            moonc build-package ./A/hello.mbt ./A/test.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/A/A.internal_test.core -pkg username/hello/A -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/A/A.internal_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.internal_test.wasm -test-mode -pkg-config-path ./A/moon.pkg.json -pkg-sources username/hello/A:./A -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moonc build-package ./A/hello.mbt ./A/test.mbt -o ./target/wasm-gc/debug/test/A/A.core -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/A/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/A/__blackbox_test_info.json --doctest-only ./A/hello.mbt --doctest-only ./A/test.mbt --target wasm-gc --pkg-name username/hello/A --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/A/__generated_driver_for_blackbox_test.mbt -doctest-only ./A/hello.mbt -doctest-only ./A/test.mbt -o ./target/wasm-gc/debug/test/A/A.blackbox_test.core -pkg username/hello/A_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/A/A.mi:A -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/A/A.core ./target/wasm-gc/debug/test/A/A.blackbox_test.core -main username/hello/A_blackbox_test -o ./target/wasm-gc/debug/test/A/A.blackbox_test.wasm -test-mode -pkg-config-path ./A/moon.pkg.json -pkg-sources username/hello/A:./A -pkg-sources username/hello/A_blackbox_test:./A -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/test/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm-gc --pkg-name username/hello/main --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/wasm-gc/debug/test/main/main.blackbox_test.core -pkg username/hello/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/main/main.mi:main -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/main/main.core ./target/wasm-gc/debug/test/main/main.blackbox_test.core -main username/hello/main_blackbox_test -o ./target/wasm-gc/debug/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources username/hello/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib2/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib2/__internal_test_info.json ./lib2/lib.mbt --target wasm-gc --pkg-name username/hello/lib2 --driver-kind internal
            moonc build-package ./lib2/lib.mbt ./target/wasm-gc/debug/test/lib2/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib2/lib2.internal_test.core -pkg username/hello/lib2 -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib2/lib2.internal_test.core -main username/hello/lib2 -o ./target/wasm-gc/debug/test/lib2/lib2.internal_test.wasm -test-mode -pkg-config-path ./lib2/moon.pkg.json -pkg-sources username/hello/lib2:./lib2 -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moonc build-package ./lib2/lib.mbt -o ./target/wasm-gc/debug/test/lib2/lib2.core -pkg username/hello/lib2 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib2/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib2/__blackbox_test_info.json --doctest-only ./lib2/lib.mbt --target wasm-gc --pkg-name username/hello/lib2 --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/lib2/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib2/lib.mbt -o ./target/wasm-gc/debug/test/lib2/lib2.blackbox_test.core -pkg username/hello/lib2_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib2_blackbox_test:./lib2 -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib2/lib2.core ./target/wasm-gc/debug/test/lib2/lib2.blackbox_test.core -main username/hello/lib2_blackbox_test -o ./target/wasm-gc/debug/test/lib2/lib2.blackbox_test.wasm -test-mode -pkg-config-path ./lib2/moon.pkg.json -pkg-sources username/hello/lib2:./lib2 -pkg-sources username/hello/lib2_blackbox_test:./lib2 -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__whitebox_test_info.json ./lib/hello_wbtest.mbt --target wasm-gc --pkg-name username/hello/lib --driver-kind whitebox
            moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -whitebox-test -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/A/__generated_driver_for_whitebox_test.mbt --output-metadata ./target/wasm-gc/debug/test/A/__whitebox_test_info.json ./A/hello_wbtest.mbt --target wasm-gc --pkg-name username/hello/A --driver-kind whitebox
            moonc build-package ./A/hello.mbt ./A/test.mbt ./A/hello_wbtest.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/A/A.whitebox_test.core -pkg username/hello/A -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -g -O0 -source-map -whitebox-test -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/A/A.whitebox_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.whitebox_test.wasm -test-mode -pkg-config-path ./A/moon.pkg.json -pkg-sources username/hello/A:./A -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/A/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/A/__internal_test_info.json ./A/hello.mbt ./A/test.mbt --target wasm-gc --pkg-name username/hello/A --driver-kind internal
            moonc build-package ./A/hello.mbt ./A/test.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/A/A.internal_test.core -pkg username/hello/A -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/A/A.internal_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.internal_test.wasm -test-mode -pkg-config-path ./A/moon.pkg.json -pkg-sources username/hello/A:./A -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moonc build-package ./A/hello.mbt ./A/test.mbt -o ./target/wasm-gc/debug/test/A/A.core -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/A/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/A/__blackbox_test_info.json --doctest-only ./A/hello.mbt --doctest-only ./A/test.mbt --target wasm-gc --pkg-name username/hello/A --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/A/__generated_driver_for_blackbox_test.mbt -doctest-only ./A/hello.mbt -doctest-only ./A/test.mbt -o ./target/wasm-gc/debug/test/A/A.blackbox_test.core -pkg username/hello/A_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/A/A.mi:A -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/A/A.core ./target/wasm-gc/debug/test/A/A.blackbox_test.core -main username/hello/A_blackbox_test -o ./target/wasm-gc/debug/test/A/A.blackbox_test.wasm -test-mode -pkg-config-path ./A/moon.pkg.json -pkg-sources username/hello/A:./A -pkg-sources username/hello/A_blackbox_test:./A -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );
}

#[test]
fn test_moon_test_filter_file() {
    let dir = TestDir::new("test_filter/test_filter");

    check(
        get_stdout(&dir, ["test", "-p", "username/hello/A", "-f", "hello.mbt"]),
        expect![[r#"
            test A
            test B
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["test", "-p", "username/hello/lib", "-f", "hello_wbtest.mbt"],
        ),
        expect![[r#"
            test hello_0
            test hello_1
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_file_index_with_path_arg() {
    let dir = TestDir::new("test_filter/test_filter");

    // Path argument form from module root
    check(
        get_stdout(&dir, ["test", "A/hello.mbt", "-i", "1"]),
        expect![[r#"
            test B
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    // Path argument form from inside package directory
    check(
        get_stdout(&dir.join("A"), ["test", "hello.mbt", "-i", "1"]),
        expect![[r#"
            test B
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_index() {
    let dir = TestDir::new("test_filter/test_filter");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "-f",
                "hello.mbt",
                "-i",
                "1",
            ],
        ),
        expect![[r#"
            test B
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib",
                "-f",
                "hello_wbtest.mbt",
                "-i",
                "0",
            ],
        ),
        expect![[r#"
            test hello_0
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_index_with_auto_update() {
    let dir = TestDir::new("test_filter/test_filter");

    let _ = get_stdout(
        &dir,
        [
            "test",
            "-p",
            "username/hello/lib2",
            "-f",
            "lib.mbt",
            "-i",
            "1",
            "-u",
            "--no-parallelize",
        ],
    );
    check(
        read(dir.join("lib2").join("lib.mbt")),
        expect![[r#"
            test {
              println(2)
            }

            test {
              inspect(1, content="1")
              inspect(1 + 2, content="3")
              inspect("hello", content="hello")
              inspect([1, 2, 3], content="[1, 2, 3]")
            }

            test {
              inspect(2)
            }
        "#]],
    );

    let dir = TestDir::new("test_filter/test_filter");
    let _ = get_err_stderr(
        &dir,
        [
            "test",
            "-p",
            "username/hello/lib2",
            "-f",
            "lib.mbt",
            "-i",
            "1",
            "-u",
            "-l",
            "2",
            "--no-parallelize",
        ],
    );
    check(
        read(dir.join("lib2").join("lib.mbt")),
        expect![[r#"
            test {
              println(2)
            }

            test {
              inspect(1, content="1")
              inspect(1 + 2, content="3")
              inspect("hello")
              inspect([1, 2, 3])
            }

            test {
              inspect(2)
            }
        "#]],
    );

    let dir = TestDir::new("test_filter/test_filter");
    let _ = get_err_stderr(
        &dir,
        [
            "test",
            "-p",
            "username/hello/lib2",
            "-f",
            "lib.mbt",
            "-u",
            "-l",
            "1",
            "--no-parallelize",
        ],
    );
    check(
        read(dir.join("lib2").join("lib.mbt")),
        expect![[r#"
            test {
              println(2)
            }

            test {
              inspect(1, content="1")
              inspect(1 + 2)
              inspect("hello")
              inspect([1, 2, 3])
            }

            test {
              inspect(2, content="2")
            }
        "#]],
    );
}

#[test]
fn moon_test_parallelize_should_success() {
    let dir = TestDir::new("test_filter/pkg_with_test_imports");

    let output = get_stdout(&dir, ["test"]);
    assert!(output.contains("Total tests: 14, passed: 14, failed: 0."));

    let output = get_stdout(&dir, ["test", "--target", "native"]);
    assert!(output.contains("Total tests: 14, passed: 14, failed: 0."));
}

#[test]
fn moon_test_parallelize_test_filter_should_success() {
    let dir = TestDir::new("test_filter/test_filter");

    let output = get_err_stdout(&dir, ["test"]);
    assert!(output.contains("Total tests: 13, passed: 11, failed: 2."));

    let output = get_err_stdout(&dir, ["test", "--target", "native"]);
    assert!(output.contains("Total tests: 13, passed: 11, failed: 2."));

    let output = get_stdout(&dir, ["test", "-u", "--no-parallelize"]);
    assert!(output.contains("Total tests: 13, passed: 13, failed: 0."));

    let output = get_stdout(
        &dir,
        ["test", "-u", "--no-parallelize", "--target", "native"],
    );
    assert!(output.contains("Total tests: 13, passed: 13, failed: 0."));
}
