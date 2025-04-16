use super::*;

#[test]
fn test_import001() {
    let dir = TestDir::new("fancy_import/import001");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
}

#[test]
fn test_import002() {
    let dir = TestDir::new("fancy_import/import002");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
}

#[test]
fn test_import003() {
    let dir = TestDir::new("fancy_import/import003");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
            Hello, world2!
        "#]],
    );
}

#[test]
fn test_import004() {
    let dir = TestDir::new("fancy_import/import004");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            f1
            f2
            f3
            f4
        "#]],
    );
}
