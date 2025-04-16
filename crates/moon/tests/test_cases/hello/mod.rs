use super::*;

#[test]
fn test_hello() {
    let dir = TestDir::new("hello");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
    check(
        dir.join("target")
            .join("common")
            .join(".moon-lock")
            .exists()
            .to_string(),
        expect!["false"],
    );
}
