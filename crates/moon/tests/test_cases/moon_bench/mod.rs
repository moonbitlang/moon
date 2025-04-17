use super::*;

#[test]
fn test_moon_bench() {
    let dir = TestDir::new("moon_bench");

    check(
        get_stdout(
            &dir,
            [
                "bench",
                "--target",
                "all",
                "--sort-input",
                "--dry-run",
                "--serial",
            ],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbench/lib --sort-input --target wasm --driver-kind internal --mode bench
            moonc build-package ./lib/hello.mbt ./lib/hello_bench.mbt ./target/wasm/debug/bench/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/bench/lib/lib.internal_test.core -pkg moonbench/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -pkg-sources moonbench/lib:./lib -target wasm -g -O0 -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm/release/bundle/core.core ./target/wasm/debug/bench/lib/lib.internal_test.core -main moonbench/lib -o ./target/wasm/debug/bench/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbench/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbench/lib --sort-input --target wasm-gc --driver-kind internal --mode bench
            moonc build-package ./lib/hello.mbt ./lib/hello_bench.mbt ./target/wasm-gc/debug/bench/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/bench/lib/lib.internal_test.core -pkg moonbench/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moonbench/lib:./lib -target wasm-gc -g -O0 -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/bench/lib/lib.internal_test.core -main moonbench/lib -o ./target/wasm-gc/debug/bench/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbench/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbench/lib --sort-input --target js --driver-kind internal --mode bench
            moonc build-package ./lib/hello.mbt ./lib/hello_bench.mbt ./target/js/debug/bench/lib/__generated_driver_for_internal_test.mbt -o ./target/js/debug/bench/lib/lib.internal_test.core -pkg moonbench/lib -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources moonbench/lib:./lib -target js -g -O0 -no-mi
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/core.core ./target/js/debug/bench/lib/lib.internal_test.core -main moonbench/lib -o ./target/js/debug/bench/lib/lib.internal_test.js -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbench/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0
        "#]],
    );
}

#[test]
fn moon_bench_parallelize_should_success() {
    let dir = TestDir::new("moon_bench");

    let output = get_stdout(&dir, ["bench", "--target", "all", "--sort-input"]);

    assert!(!output.contains("bench moonbench/lib/hello_bench.mbt::non-bench"));
    assert!(output.contains("bench moonbench/lib/hello_bench.mbt::bench"));
    assert!(output.contains("bench moonbench/lib/hello_bench.mbt::bench: naive fib"));
    assert!(output.contains("time "));
    assert!(output.contains("range "));
}

#[test]
fn moon_bench_native_parallelize_should_success() {
    let dir = TestDir::new("moon_bench");

    let output = get_stdout(&dir, ["bench", "--target", "native", "--sort-input"]);

    assert!(!output.contains("bench moonbench/lib/hello_bench.mbt::non-bench"));
    assert!(output.contains("bench moonbench/lib/hello_bench.mbt::bench"));
    assert!(output.contains("bench moonbench/lib/hello_bench.mbt::bench: naive fib"));
    assert!(output.contains("time "));
    assert!(output.contains("range "));
}

#[test]
fn moon_test_on_bench_prefixed() {
    let dir = TestDir::new("moon_bench");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "js",
                "--sort-input",
                "--no-parallelize",
                "-p",
                "moonbench/lib",
                "-f",
                "hello_bench.mbt",
                "-i",
                "0",
            ],
        ),
        expect!([r#"
            Total tests: 0, passed: 0, failed: 0.
        "#]),
    );
}

#[test]
fn moon_bench_on_non_bench_prefixed() {
    let dir = TestDir::new("moon_bench");

    check(
        get_stdout(
            &dir,
            [
                "bench",
                "--target",
                "js",
                "--sort-input",
                "--serial",
                "-p",
                "moonbench/lib",
                "-f",
                "hello_bench.mbt",
                "-i",
                "1",
            ],
        ),
        expect!([r#"
        Total tests: 0, passed: 0, failed: 0.
    "#]),
    );
}

#[test]
fn moon_bench_on_no_args_test() {
    let dir = TestDir::new("moon_bench");

    check(
        get_stdout(
            &dir,
            [
                "bench",
                "--target",
                "js",
                "--sort-input",
                "--serial",
                "-p",
                "moonbench/lib",
                "-f",
                "hello_bench.mbt",
                "-i",
                "4",
            ],
        ),
        expect!([r#"
            Total tests: 0, passed: 0, failed: 0.
        "#]),
    )
}

#[test]
fn moon_bench_filter_bench() {
    let dir = TestDir::new("moon_bench");

    let output = get_stdout(
        &dir,
        [
            "bench",
            "--target",
            "all",
            "--sort-input",
            "--serial",
            "-p",
            "moonbench/lib",
            "-f",
            "hello_bench.mbt",
            "-i",
            "3",
        ],
    );
    assert!(output.contains("bench moonbench/lib/hello_bench.mbt::bench: naive fib"));
    assert!(output.contains("time "));
    assert!(output.contains("range "));
}
