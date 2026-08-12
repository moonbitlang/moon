use crate::test_cases::*;
use crate::util::check;

#[test]
fn native_backend_test_filter() {
    let dir = TestDir::new("native_backend/test_filter");
    let source = dir.join("lib/hello.mbt");
    assert!(!read(&source).contains("content=(#|523"));

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "native",
                "-p",
                "lib",
                "--file",
                "hello.mbt",
                "-i",
                "1",
                "--update",
                "--sort-input",
            ],
        ),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        read(source),
        expect![[r#"
            pub fn hello() -> String {
              "Hello, world!"
            }

            test "A" {
              println("test A")
            }

            test "inspect" {
              inspect(523, content=(#|523
              ))
            }
        "#]],
    );
}
