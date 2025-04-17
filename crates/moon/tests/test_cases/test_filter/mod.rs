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
            test A
            test B
            test C
            test D
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
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/A --sort-input --target wasm-gc --driver-kind whitebox --mode test
            moonc build-package ./A/hello.mbt ./A/test.mbt ./A/hello_wbtest.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/A/A.whitebox_test.core -pkg username/hello/A -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -g -O0 -whitebox-test -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/A/A.whitebox_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.whitebox_test.wasm -test-mode -pkg-config-path ./A/moon.pkg.json -pkg-sources username/hello/A:./A -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/A --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./A/hello.mbt ./A/test.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/A/A.internal_test.core -pkg username/hello/A -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -g -O0 -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/A/A.internal_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.internal_test.wasm -test-mode -pkg-config-path ./A/moon.pkg.json -pkg-sources username/hello/A:./A -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib2 --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./lib2/lib.mbt ./target/wasm-gc/debug/test/lib2/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib2/lib2.internal_test.core -pkg username/hello/lib2 -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -g -O0 -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib2/lib2.internal_test.core -main username/hello/lib2 -o ./target/wasm-gc/debug/test/lib2/lib2.internal_test.wasm -test-mode -pkg-config-path ./lib2/moon.pkg.json -pkg-sources username/hello/lib2:./lib2 -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target wasm-gc --driver-kind whitebox --mode test
            moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -whitebox-test -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/A --sort-input --target wasm-gc --driver-kind whitebox --mode test
            moonc build-package ./A/hello.mbt ./A/test.mbt ./A/hello_wbtest.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/A/A.whitebox_test.core -pkg username/hello/A -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -g -O0 -whitebox-test -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/A/A.whitebox_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.whitebox_test.wasm -test-mode -pkg-config-path ./A/moon.pkg.json -pkg-sources username/hello/A:./A -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/A --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./A/hello.mbt ./A/test.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/A/A.internal_test.core -pkg username/hello/A -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -g -O0 -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/A/A.internal_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.internal_test.wasm -test-mode -pkg-config-path ./A/moon.pkg.json -pkg-sources username/hello/A:./A -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
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
              inspect!(1, content="1")
              inspect!(1 + 2, content="3")
              inspect!("hello", content="hello")
              inspect!([1, 2, 3], content="[1, 2, 3]")
            }

            test {
              inspect!(2)
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
              inspect!(1, content="1")
              inspect!(1 + 2, content="3")
              inspect!("hello")
              inspect!([1, 2, 3])
            }

            test {
              inspect!(2)
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
              inspect!(1, content="1")
              inspect!(1 + 2)
              inspect!("hello")
              inspect!([1, 2, 3])
            }

            test {
              inspect!(2, content="2")
            }
        "#]],
    );
}
