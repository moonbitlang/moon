use expect_test::expect;

use crate::{
    TestDir, get_stdout,
    util::{check, moon_bin},
};

#[test]
fn debug_flag_test() {
    let dir = TestDir::new("debug_flag_test");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();

    check(
        get_stdout(&dir, ["check", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc check ./lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -workspace-path .
            moonc check ./main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg hello/main -is-main -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -workspace-path .
        "#]],
    );

    check(
        get_stdout(&dir, ["build", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -workspace-path .
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
        "#]],
    );

    check(
        get_stdout(&dir, ["build", "--dry-run", "--debug", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(&dir, ["run", "main", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -workspace-path .
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm
        "#]],
    );

    check(
        get_stdout(&dir, ["run", "main", "--dry-run", "--debug", "--nostd"]),
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
            ["build", "--target", "wasm-gc", "--dry-run", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -workspace-path .
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--dry-run",
                "--target",
                "wasm-gc",
                "--debug",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["run", "main", "--target", "wasm-gc", "--dry-run", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -workspace-path .
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "run",
                "main",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--debug",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
            moonrun ./target/wasm-gc/debug/build/main/main.wasm
        "#]],
    );

    // release should conflict with debug
    #[cfg(unix)]
    {
        use crate::get_err_stderr;

        check(
            get_err_stderr(&dir, ["test", "--release", "--debug"]),
            expect![[r#"
                error: the argument '--release' cannot be used with '--debug'

                Usage: moon test --release [PATH]

                For more information, try '--help'.
            "#]],
        );

        check(
            get_err_stderr(&dir, ["build", "--debug", "--release"]),
            expect![[r#"
                error: the argument '--debug' cannot be used with '--release'

                Usage: moon build --debug [PATH]

                For more information, try '--help'.
            "#]],
        );

        check(
            get_err_stderr(&dir, ["check", "--release", "--debug"]),
            expect![[r#"
                error: the argument '--release' cannot be used with '--debug'

                Usage: moon check --release [PATH]

                For more information, try '--help'.
            "#]],
        );

        check(
            get_err_stderr(&dir, ["run", "main", "--debug", "--release"]),
            expect![[r#"
                error: the argument '--debug' cannot be used with '--release'

                Usage: moon run --debug <PACKAGE_OR_MBT_FILE> [ARGS]...

                For more information, try '--help'.
            "#]],
        );
    }
}
