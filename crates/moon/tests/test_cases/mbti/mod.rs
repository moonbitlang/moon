use std::io::Write;

use super::*;
use moonutil::constants::MBTI_GENERATED;

#[test]
fn test_mbti() {
    let dir = TestDir::new("mbti");
    let _ = get_stdout(&dir, ["info"]);
    let lib_mi_out = &std::fs::read_to_string(dir.join("lib").join(MBTI_GENERATED)).unwrap();
    expect![[r#"
        // Generated using `moon info`, DON'T EDIT IT
        package "username/hello/lib"

        import {
          "moonbitlang/core/list",
        }

        // Values
        pub fn hello() -> String

        pub let hello_list : @list.List[String]

        // Errors

        // Types and methods

        // Type aliases

        // Traits
    "#]]
    .assert_eq(lib_mi_out);

    let main_mi_out = &std::fs::read_to_string(dir.join("main").join(MBTI_GENERATED)).unwrap();
    expect![[r#"
        // Generated using `moon info`, DON'T EDIT IT
        package "username/hello/main"

        // Values

        // Errors

        // Types and methods

        // Type aliases

        // Traits
    "#]]
    .assert_eq(main_mi_out);
}

#[test]
fn test_mbti_no_alias() {
    let dir = TestDir::new("mbti");
    let _ = get_stdout(&dir, ["info", "--no-alias"]);
    let lib_mi_out = &std::fs::read_to_string(dir.join("lib").join(MBTI_GENERATED)).unwrap();
    expect![[r#"
        // Generated using `moon info`, DON'T EDIT IT
        package "username/hello/lib"

        // Values
        pub fn hello() -> String

        pub let hello_list : @moonbitlang/core/list.List[String]

        // Errors

        // Types and methods

        // Type aliases

        // Traits
    "#]]
    .assert_eq(lib_mi_out);

    let main_mi_out = &std::fs::read_to_string(dir.join("main").join(MBTI_GENERATED)).unwrap();
    expect![[r#"
        // Generated using `moon info`, DON'T EDIT IT
        package "username/hello/main"

        // Values

        // Errors

        // Types and methods

        // Type aliases

        // Traits
    "#]]
    .assert_eq(main_mi_out);
}

fn modified_time(path: &std::path::Path) -> std::time::SystemTime {
    std::fs::metadata(path).unwrap().modified().unwrap()
}

#[test]
fn test_check_publishes_mbti() {
    let dir = TestDir::new("mbti");
    let lib_mbti = dir.join("lib").join(MBTI_GENERATED);
    assert_success(&dir, ["check"]);
    let checked = std::fs::read_to_string(&lib_mbti).unwrap();

    // `moon check` publishes the same interface as `moon info`.
    let info_dir = TestDir::new("mbti");
    assert_success(&info_dir, ["info"]);
    let info = std::fs::read_to_string(info_dir.join("lib").join(MBTI_GENERATED)).unwrap();
    assert_eq!(checked, info);

    // An up-to-date interface is not rewritten.
    let published_at = modified_time(&lib_mbti);
    assert_success(&dir, ["check"]);
    assert_eq!(modified_time(&lib_mbti), published_at);

    // A changed interface is published again.
    let mut source = std::fs::OpenOptions::new()
        .append(true)
        .open(dir.join("lib").join("hello.mbt"))
        .unwrap();
    writeln!(source, "\npub fn goodbye() -> String {{ \"bye\" }}").unwrap();
    assert_success(&dir, ["check"]);
    let updated = std::fs::read_to_string(&lib_mbti).unwrap();
    assert!(updated.contains("pub fn goodbye() -> String"), "{updated}");
}

#[test]
fn test_check_does_not_publish_mbti() {
    let dir = TestDir::new("mbti");
    let lib_mbti = dir.join("lib").join(MBTI_GENERATED);

    assert_success(&dir, ["check", "--no-mbti"]);
    assert!(!lib_mbti.exists());

    // Only the package's preferred backend owns its published interface.
    assert_success(&dir, ["check", "--target", "js"]);
    assert!(!lib_mbti.exists());

    // A failed check publishes nothing.
    let mut source = std::fs::OpenOptions::new()
        .append(true)
        .open(dir.join("lib").join("hello.mbt"))
        .unwrap();
    writeln!(source, "\npub fn broken() -> Int {{ \"not an int\" }}").unwrap();
    let _ = get_err_stderr(&dir, ["check"]);
    assert!(!lib_mbti.exists());
}
