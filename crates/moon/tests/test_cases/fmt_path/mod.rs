use super::*;

#[test]
fn test_fmt_path() {
    let dir = TestDir::new("fmt_path.in");

    let _ = get_stdout(&dir, ["fmt", "."]);
    check(
        read(dir.join("fmt_path.mbt")),
        expect![[r#"
            ///|
            pub fn fib(n : Int) -> Int {
              n
            }
        "#]],
    );
    check(
        read(dir.join("cmd/main/main.mbt")),
        expect![[r#"
            ///|
            fn main { println(@lib.fib(10)) }
        "#]],
    );

    let _ = get_stdout(&dir, ["fmt", "cmd/main"]);
    check(
        read(dir.join("cmd/main/main.mbt")),
        expect![[r#"
            ///|
            fn main {
              println(@lib.fib(10))
            }
        "#]],
    );
}
