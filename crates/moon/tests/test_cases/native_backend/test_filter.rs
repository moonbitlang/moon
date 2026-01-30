use crate::test_cases::*;
use crate::util::check;

#[test]
fn native_backend_test_filter() {
    let dir = TestDir::new("native_backend/test_filter");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "native",
                "-p",
                "lib",
                "-f",
                "hello.mbt",
                "-i",
                "3",
                "--sort-input",
            ],
        ),
        expect![[r#"
            test C
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    // This test updates the expect test for input 2
    let file = dir.join("lib/hello.mbt");
    let original_content = read(&file);
    println!("Original content:\n{}", original_content);

    assert!(!original_content.contains("content=\"523\""));
    assert!(!original_content.contains("content=\"asdfhjas\""));
    get_stdout(
        &dir,
        [
            "test",
            "--target",
            "native",
            "-p",
            "lib",
            "-f",
            "hello.mbt",
            "-i",
            "2",
            "-u",
            "--sort-input",
        ],
    );
    let updated_content = read(&file);
    println!("Updated content:\n{}", updated_content);
    assert!(updated_content.contains("content=(#|523"));
    assert!(updated_content.contains("content=(#|asdfhjas"));

    let file = dir.join("lib/hello_wbtest.mbt");
    let original_content = read(&file);
    println!("Original content:\n{}", original_content);

    assert!(!original_content.contains("content=\"1256\""));
    get_stdout(
        &dir,
        [
            "test",
            "--target",
            "native",
            "-p",
            "lib",
            "-f",
            "hello_wbtest.mbt",
            "-i",
            "1",
            "-u",
            "--sort-input",
        ],
    );
    let updated_content = read(&file);
    println!("Updated content:\n{}", updated_content);
    assert!(updated_content.contains("content=(#|1256"));

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "native",
                "-p",
                "lib",
                "-f",
                "hello_wbtest.mbt",
                "-i",
                "0",
                "--sort-input",
            ],
        ),
        expect![[r#"
            test hello_0
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_err_stdout(
            &dir,
            [
                "test",
                "--target",
                "native",
                "-p",
                "lib",
                "-f",
                "hello.mbt",
                "-i",
                "4",
                "--sort-input",
            ],
        ),
        expect![[r#"
            [username/hello] test lib/hello.mbt:24 ("D") failed
            expect test failed at $ROOT/lib/hello.mbt:26:3
            Diff: (- expected, + actual)
            ----
            +test D
            ----

            Total tests: 1, passed: 0, failed: 1.
        "#]],
    );

    let file = dir.join("lib/__snapshot__/test.d");
    assert!(!file.exists());
    get_stdout(
        &dir,
        [
            "test",
            "--target",
            "native",
            "-p",
            "lib",
            "-f",
            "hello.mbt",
            "-i",
            "4",
            "-u",
            "--sort-input",
        ],
    );
    let updated_content = read(&file);
    println!("Updated content:\n{}", updated_content);
    assert!(updated_content.contains("test D"));
}
