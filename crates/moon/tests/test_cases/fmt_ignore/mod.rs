use super::*;

#[test]
fn test_moon_fmt_ignore() {
    let dir = TestDir::new("fmt_ignore");

    // Initial check of unformatted files
    let ignored_file = dir.join("lib").join("hello_ignore.mbt");
    let not_ignored_file = dir.join("lib").join("hello.mbt");

    let ignored_original_contents = read(&ignored_file);
    let not_ignored_original_contents = read(&not_ignored_file);

    expect![[r#"
        pub fn hello_ignore() -> String { "Hello, world!" }
    "#]]
    .assert_eq(&ignored_original_contents);
    expect![[r#"
        pub fn hello() -> String { "Hello, world!" }
    "#]]
    .assert_eq(&not_ignored_original_contents);

    // Run formatter
    let _ = get_stdout(&dir, ["fmt"]);

    // Check that ignored file is unchanged
    let ignored_after_fmt_contents = read(&ignored_file);
    assert_eq!(ignored_original_contents, ignored_after_fmt_contents);

    // Check that not ignored file is formatted
    let not_ignored_after_fmt_contents = read(&not_ignored_file);
    assert_ne!(
        not_ignored_original_contents,
        not_ignored_after_fmt_contents
    );
    expect![[r#"
        ///|
        pub fn hello() -> String {
          "Hello, world!"
        }
    "#]]
    .assert_eq(&not_ignored_after_fmt_contents);
}
