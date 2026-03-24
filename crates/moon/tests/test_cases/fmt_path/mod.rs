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

#[test]
fn test_fmt_multiple_paths_skip_filtered_entries() {
    let dir = TestDir::new("fmt_path.in");
    std::fs::create_dir_all(dir.join("notes")).unwrap();
    std::fs::write(dir.join("notes/README.txt"), "not a package").unwrap();

    let _ = get_stdout(&dir, ["fmt", "fmt_path.mbt", "cmd/main", "notes"]);
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
            fn main {
              println(@lib.fib(10))
            }
        "#]],
    );

    let stderr = get_stderr(
        &dir,
        ["fmt", "fmt_path.mbt", "cmd/main", "notes", "--verbose"],
    );
    assert!(stderr.contains("skipping path `notes`"), "stderr: {stderr}");
}
