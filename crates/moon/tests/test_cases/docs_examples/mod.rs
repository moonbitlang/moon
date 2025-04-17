use super::*;

#[test]
fn test_unicode_demo() {
    let dir = TestDir::new("docs_examples/unicode_demo");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            3
        "#]],
    );
}

#[test]
fn test_palindrome_string() {
    let dir = TestDir::new("docs_examples/palindrome_string");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
        aba
    "#]],
    );
}

#[test]
fn test_avl_tree() {
    let dir = TestDir::new("docs_examples/avl_tree");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            height of the tree: 6
                    0
                  1
                    2
                3
                    4
                  5
                    6
              7
                    8
                  9
                    10
                11
                    12
                  13
                    14
            15
                    16
                  17
                    18
                19
                    20
                  21
                    22
              23
                  24
                25
                    26
                  27
                    28
                      29
            success
        "#]],
    );
}

#[test]
fn test_docstring_demo() {
    let dir = TestDir::new("docs_examples/docstring_demo");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
}

#[test]
fn test_multidimensional_arrays() {
    let dir = TestDir::new("docs_examples/multidimensional_arrays");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
        11
    "#]],
    );
}
