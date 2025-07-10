use super::*;

#[test]
#[cfg(unix)]
fn test_mbti() {
    let dir = TestDir::new("mbti");
    let _ = get_stdout(&dir, ["info"]);
    let lib_mi_out = &std::fs::read_to_string(dir.join("lib").join("lib.mbti")).unwrap();
    expect![[r#"
        // Generated using `moon info`, DON'T EDIT IT
        package "username/hello/lib"

        import(
          "moonbitlang/core/immut/list"
        )

        // Values
        fn hello() -> String

        let hello_list : @list.T[String]

        // Types and methods

        // Type aliases

        // Traits

    "#]]
    .assert_eq(lib_mi_out);

    let main_mi_out = &std::fs::read_to_string(dir.join("main").join("main.mbti")).unwrap();
    expect![[r#"
        // Generated using `moon info`, DON'T EDIT IT
        package "username/hello/main"

        // Values

        // Types and methods

        // Type aliases

        // Traits

    "#]]
    .assert_eq(main_mi_out);
}

#[test]
#[cfg(unix)]
fn test_mbti_no_alias() {
    let dir = TestDir::new("mbti");
    let _ = get_stdout(&dir, ["info", "--no-alias"]);
    let lib_mi_out = &std::fs::read_to_string(dir.join("lib").join("lib.mbti")).unwrap();
    expect![[r#"
        // Generated using `moon info`, DON'T EDIT IT
        package "username/hello/lib"

        // Values
        fn hello() -> String

        let hello_list : @moonbitlang/core/immut/list.T[String]

        // Types and methods

        // Type aliases

        // Traits

    "#]]
    .assert_eq(lib_mi_out);

    let main_mi_out = &std::fs::read_to_string(dir.join("main").join("main.mbti")).unwrap();
    expect![[r#"
        // Generated using `moon info`, DON'T EDIT IT
        package "username/hello/main"

        // Values

        // Types and methods

        // Type aliases

        // Traits

    "#]]
    .assert_eq(main_mi_out);
}
