use super::*;

#[test]
fn test_moon_fmt() {
    let dir = TestDir::new("fmt");
    check(
        read(dir.join("lib").join("hello.mbt")),
        expect![[r#"
                pub fn hello() -> String { "Hello, world!" }
            "#]],
    );
    check(
        read(dir.join("main").join("main.mbt")),
        expect![[r#"
                fn main { println(@lib.hello()) }"#]],
    );
    check(
        read(dir.join("lib").join("test.mbt.md")),
        expect![[r#"
        This is for testing formatter on `.mbt.md` files

        ```mbt
        fn __test_formatter() ->Unit{ println("hell world")             }
        ```"#]],
    );
    let _ = get_stdout(&dir, ["fmt"]);
    check(
        read(dir.join("lib").join("hello.mbt")),
        expect![[r#"
            ///|
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );
    check(
        read(dir.join("main").join("main.mbt")),
        expect![[r#"
            ///|
            fn main {
              println(@lib.hello())
            }
        "#]],
    );
    check(
        read(dir.join("lib").join("test.mbt.md")),
        expect![[r#"
            This is for testing formatter on `.mbt.md` files

            ```mbt
            ///|
            fn __test_formatter() -> Unit {
              println("hell world")
            }
            ```
        "#]],
    );
}

#[cfg(unix)]
#[test]
fn test_moon_fmt_002() {
    let dir = TestDir::new("fmt");
    let _ = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["fmt", "--check"])
        .assert()
        .failure()
        .get_output()
        .stdout
        .to_owned();
    check(
        read(dir.join("lib").join("hello.mbt")),
        expect![[r#"
            pub fn hello() -> String { "Hello, world!" }
        "#]],
    );
    check(
        read(dir.join("main").join("main.mbt")),
        expect![[r#"
            fn main { println(@lib.hello()) }"#]],
    );
    check(
        read(
            dir.join("target")
                .join(TargetBackend::default().to_dir_name())
                .join("release")
                .join("format")
                .join("lib")
                .join("hello.mbt"),
        ),
        expect![[r#"
            ///|
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );
    check(
        read(
            dir.join("target")
                .join(TargetBackend::default().to_dir_name())
                .join("release")
                .join("format")
                .join("main")
                .join("main.mbt"),
        ),
        expect![[r#"
            ///|
            fn main {
              println(@lib.hello())
            }
        "#]],
    );
}

#[test]
fn test_moon_fmt_extra_args() {
    let dir = TestDir::new("fmt");
    check(
        get_stdout(&dir, ["fmt", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonfmt ./lib/test.mbt.md -w -o ./target/wasm-gc/release/format/lib/test.mbt.md
            moonfmt ./main/main.mbt -w -o ./target/wasm-gc/release/format/main/main.mbt
            moonfmt ./lib/hello_wbtest.mbt -w -o ./target/wasm-gc/release/format/lib/hello_wbtest.mbt
            moonfmt ./lib/hello.mbt -w -o ./target/wasm-gc/release/format/lib/hello.mbt
        "#]],
    );
    check(
        get_stdout(&dir, ["fmt", "--dry-run", "--sort-input", "--", "a", "b"]),
        expect![[r#"
            moonfmt ./lib/test.mbt.md -w -o ./target/wasm-gc/release/format/lib/test.mbt.md a b
            moonfmt ./main/main.mbt -w -o ./target/wasm-gc/release/format/main/main.mbt a b
            moonfmt ./lib/hello_wbtest.mbt -w -o ./target/wasm-gc/release/format/lib/hello_wbtest.mbt a b
            moonfmt ./lib/hello.mbt -w -o ./target/wasm-gc/release/format/lib/hello.mbt a b
        "#]],
    );
    check(
        get_stdout(&dir, ["fmt", "--check", "--sort-input", "--dry-run"]),
        expect![[r#"
            moon tool format-and-diff --old ./lib/test.mbt.md --new ./target/wasm-gc/release/format/lib/test.mbt.md
            moon tool format-and-diff --old ./main/main.mbt --new ./target/wasm-gc/release/format/main/main.mbt
            moon tool format-and-diff --old ./lib/hello_wbtest.mbt --new ./target/wasm-gc/release/format/lib/hello_wbtest.mbt
            moon tool format-and-diff --old ./lib/hello.mbt --new ./target/wasm-gc/release/format/lib/hello.mbt
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "fmt",
                "--check",
                "--sort-input",
                "--dry-run",
                "--",
                "c",
                "d",
            ],
        ),
        expect![[r#"
            moon tool format-and-diff --old ./lib/test.mbt.md --new ./target/wasm-gc/release/format/lib/test.mbt.md c d
            moon tool format-and-diff --old ./main/main.mbt --new ./target/wasm-gc/release/format/main/main.mbt c d
            moon tool format-and-diff --old ./lib/hello_wbtest.mbt --new ./target/wasm-gc/release/format/lib/hello_wbtest.mbt c d
            moon tool format-and-diff --old ./lib/hello.mbt --new ./target/wasm-gc/release/format/lib/hello.mbt c d
        "#]],
    );
}

#[test]
fn test_moon_fmt_block_style() {
    let dir = TestDir::new("fmt");
    check(
        get_stdout(&dir, ["fmt", "--block-style", "--sort-input", "--dry-run"]),
        expect![[r#"
            moonfmt ./lib/test.mbt.md -w -o ./target/wasm-gc/release/format/lib/test.mbt.md -block-style
            moonfmt ./main/main.mbt -w -o ./target/wasm-gc/release/format/main/main.mbt -block-style
            moonfmt ./lib/hello_wbtest.mbt -w -o ./target/wasm-gc/release/format/lib/hello_wbtest.mbt -block-style
            moonfmt ./lib/hello.mbt -w -o ./target/wasm-gc/release/format/lib/hello.mbt -block-style
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["fmt", "--block-style=true", "--sort-input", "--dry-run"],
        ),
        expect![[r#"
            moonfmt ./lib/test.mbt.md -w -o ./target/wasm-gc/release/format/lib/test.mbt.md -block-style
            moonfmt ./main/main.mbt -w -o ./target/wasm-gc/release/format/main/main.mbt -block-style
            moonfmt ./lib/hello_wbtest.mbt -w -o ./target/wasm-gc/release/format/lib/hello_wbtest.mbt -block-style
            moonfmt ./lib/hello.mbt -w -o ./target/wasm-gc/release/format/lib/hello.mbt -block-style
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["fmt", "--block-style=false", "--sort-input", "--dry-run"],
        ),
        expect![[r#"
            moonfmt ./lib/test.mbt.md -w -o ./target/wasm-gc/release/format/lib/test.mbt.md
            moonfmt ./main/main.mbt -w -o ./target/wasm-gc/release/format/main/main.mbt
            moonfmt ./lib/hello_wbtest.mbt -w -o ./target/wasm-gc/release/format/lib/hello_wbtest.mbt
            moonfmt ./lib/hello.mbt -w -o ./target/wasm-gc/release/format/lib/hello.mbt
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "fmt",
                "--block-style",
                "--check",
                "--sort-input",
                "--dry-run",
            ],
        ),
        expect![[r#"
            moon tool format-and-diff --old ./lib/test.mbt.md --new ./target/wasm-gc/release/format/lib/test.mbt.md --block-style
            moon tool format-and-diff --old ./main/main.mbt --new ./target/wasm-gc/release/format/main/main.mbt --block-style
            moon tool format-and-diff --old ./lib/hello_wbtest.mbt --new ./target/wasm-gc/release/format/lib/hello_wbtest.mbt --block-style
            moon tool format-and-diff --old ./lib/hello.mbt --new ./target/wasm-gc/release/format/lib/hello.mbt --block-style
        "#]],
    );
}
