use super::*;

#[test]
fn test_a_001() {
    let dir = TestDir::new("simple_pkg/A-001");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            A
            main
        "#]],
    );
}

#[test]
fn test_a_002() {
    let dir = TestDir::new("simple_pkg/A-002");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            A
            main
        "#]],
    );
}

#[test]
fn test_a_003() {
    let dir = TestDir::new("simple_pkg/A-003");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            A
            main
        "#]],
    );
}

#[test]
fn test_a_004() {
    let dir = TestDir::new("simple_pkg/A-004");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            A
            main
        "#]],
    );
}

#[test]
fn test_a_005() {
    let dir = TestDir::new("simple_pkg/A-005");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            A
            main
        "#]],
    );
}

#[test]
fn test_a_006() {
    let dir = TestDir::new("simple_pkg/A-006");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            main
        "#]],
    );
}

#[test]
fn test_ab_001() {
    let dir = TestDir::new("simple_pkg/AB-001");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            A
            B
            main
        "#]],
    );
}

#[test]
fn test_ab_002() {
    let dir = TestDir::new("simple_pkg/AB-002");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            A
            B
            main
        "#]],
    );
}

#[test]
fn test_ab_003() {
    let dir = TestDir::new("simple_pkg/AB-003");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            A
            B
            main
        "#]],
    );
}

#[test]
fn test_ab_004() {
    let dir = TestDir::new("simple_pkg/AB-004");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            A
            B
            main
        "#]],
    );
}
