use expect_test::expect;

use crate::{TestDir, get_stderr, get_stdout, moon_cmd, util::check};

#[test]
fn test_moon_check_legacy_json_lines_output() {
    let dir = TestDir::new("warns/deny_warn");

    check(
        get_stdout(
            &dir,
            ["check", "--output-json", "--sort-input", "-j1", "-q"],
        ),
        expect![[r#"
            {"$message_type":"diagnostic","level":"warning","error_code":2,"path":"$ROOT/lib/hello.mbt","loc":"4:7-4:8","message":"Warning (unused_value): Unused variable 'a'","context":"3 |fn _a() -> Unit {/n4 |  let a = 1;/n5 |  // 中文中文中文中文中文中文/n"}
            {"$message_type":"diagnostic","level":"warning","error_code":2,"path":"$ROOT/lib/hello.mbt","loc":"11:7-11:9","message":"Warning (unused_value): Unused variable '中文'","context":"10 |  // 🤣😭🤣😭🤣😭🤣😭🤣😭/n11 |  let 中文 = 2/n12 |  let 🤣😭🤣😭🤣 = 2/n"}
            {"$message_type":"diagnostic","level":"warning","error_code":2,"path":"$ROOT/lib/hello.mbt","loc":"12:7-12:12","message":"Warning (unused_value): Unused variable '🤣😭🤣😭🤣'","context":"11 |  let 中文 = 2/n12 |  let 🤣😭🤣😭🤣 = 2/n13 |  alert_1();/n"}
            {"$message_type":"diagnostic","level":"warning","error_code":2,"path":"$ROOT/main/main.mbt","loc":"2:7-2:8","message":"Warning (unused_value): Unused variable 'a'","context":"1 |fn main {/n2 |  let a = 0/n3 |  @lib.hello()/n"}
        "#]],
    );
}

#[test]
fn test_moon_check_json_output() {
    let dir = TestDir::new("warns/deny_warn");

    moon_cmd(&dir)
        .args(["check", "--json", "--sort-input", "-j1", "-q"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
[{"$message_type":"diagnostic","level":"warning","error_code":2,"path":"[..]/lib/hello.mbt","loc":"4:7-4:8","message":"Warning (unused_value): Unused variable 'a'","context":"3 |fn _a() -> Unit {/n4 |  let a = 1;/n5 |  // 中文中文中文中文中文中文/n"},{"$message_type":"diagnostic","level":"warning","error_code":2,"path":"[..]/lib/hello.mbt","loc":"11:7-11:9","message":"Warning (unused_value): Unused variable '中文'","context":"10 |  // 🤣😭🤣😭🤣😭🤣😭🤣😭/n11 |  let 中文 = 2/n12 |  let 🤣😭🤣😭🤣 = 2/n"},{"$message_type":"diagnostic","level":"warning","error_code":2,"path":"[..]/lib/hello.mbt","loc":"12:7-12:12","message":"Warning (unused_value): Unused variable '🤣😭🤣😭🤣'","context":"11 |  let 中文 = 2/n12 |  let 🤣😭🤣😭🤣 = 2/n13 |  alert_1();/n"},{"$message_type":"diagnostic","level":"warning","error_code":2,"path":"[..]/main/main.mbt","loc":"2:7-2:8","message":"Warning (unused_value): Unused variable 'a'","context":"1 |fn main {/n2 |  let a = 0/n3 |  @lib.hello()/n"}]

"#]])
        .stderr_eq("");
}

#[test]
fn test_moon_check_json_output_is_an_empty_array_without_diagnostics() {
    let dir = TestDir::new("moon_new/plain");

    moon_cmd(&dir)
        .args(["check", "--json", "-q"])
        .assert()
        .success()
        .stdout_eq("[]\n")
        .stderr_eq("");
}

#[test]
fn test_moon_check_json_output_preserves_diagnostics_on_failure() {
    let dir = TestDir::new("check_failed_should_write_pkg_json.in");

    moon_cmd(&dir)
        .args(["check", "--json", "--sort-input", "-j1", "-q"])
        .assert()
        .code(1)
        .stdout_eq(snapbox::str![[r#"
[{"$message_type":"diagnostic","level":"error","error_code":4014,"path":"[..]/lib/hello.mbt","loc":"2:3-2:4","message":"Expr Type Mismatch/n        has type : Int/n        wanted   : String","context":"1 |pub fn hello() -> String {/n2 |  1/n3 |}/n"}]

"#]])
        .stderr_eq("");
}

#[test]
fn test_moon_check_json_lines_output() {
    let dir = TestDir::new("warns/deny_warn");

    moon_cmd(&dir)
        .args(["check", "--jsonl", "--sort-input", "-j1", "-q"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
{"$message_type":"diagnostic","level":"warning","error_code":2,"path":"[..]/lib/hello.mbt","loc":"4:7-4:8","message":"Warning (unused_value): Unused variable 'a'","context":"3 |fn _a() -> Unit {/n4 |  let a = 1;/n5 |  // 中文中文中文中文中文中文/n"}
{"$message_type":"diagnostic","level":"warning","error_code":2,"path":"[..]/lib/hello.mbt","loc":"11:7-11:9","message":"Warning (unused_value): Unused variable '中文'","context":"10 |  // 🤣😭🤣😭🤣😭🤣😭🤣😭/n11 |  let 中文 = 2/n12 |  let 🤣😭🤣😭🤣 = 2/n"}
{"$message_type":"diagnostic","level":"warning","error_code":2,"path":"[..]/lib/hello.mbt","loc":"12:7-12:12","message":"Warning (unused_value): Unused variable '🤣😭🤣😭🤣'","context":"11 |  let 中文 = 2/n12 |  let 🤣😭🤣😭🤣 = 2/n13 |  alert_1();/n"}
{"$message_type":"diagnostic","level":"warning","error_code":2,"path":"[..]/main/main.mbt","loc":"2:7-2:8","message":"Warning (unused_value): Unused variable 'a'","context":"1 |fn main {/n2 |  let a = 0/n3 |  @lib.hello()/n"}

"#]])
        .stderr_eq("");
}

#[test]
fn test_moon_check_json_formats_are_mutually_exclusive() {
    let dir = TestDir::new("warns/deny_warn");

    moon_cmd(&dir)
        .args(["check", "--json", "--jsonl"])
        .assert()
        .code(2)
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
error: the argument '--json' cannot be used with '--jsonl'

Usage: moon check --json [PATH]...

For more information, try '--help'.

"#]]);
}

#[test]
fn test_moon_check_rendered_output() {
    let dir = TestDir::new("warns/deny_warn");

    check(
        get_stderr(&dir, ["check", "--sort-input", "-j1", "-q"]),
        expect![[r#"
            Warning: [0002]
               ╭─[ $ROOT/lib/hello.mbt:4:7 ]
               │
             4 │   let a = 1;
               │       ┬  
               │       ╰── Warning (unused_value): Unused variable 'a'
            ───╯
            Warning: [0002]
                ╭─[ $ROOT/lib/hello.mbt:11:7 ]
                │
             11 │   let 中文 = 2
                │       ──┬─  
                │         ╰─── Warning (unused_value): Unused variable '中文'
            ────╯
            Warning: [0002]
                ╭─[ $ROOT/lib/hello.mbt:12:7 ]
                │
             12 │   let 🤣😭🤣😭🤣 = 2
                │       ────┬─────  
                │           ╰─────── Warning (unused_value): Unused variable '🤣😭🤣😭🤣'
            ────╯
            Warning: [0002]
               ╭─[ $ROOT/main/main.mbt:2:7 ]
               │
             2 │   let a = 0
               │       ┬  
               │       ╰── Warning (unused_value): Unused variable 'a'
            ───╯
        "#]],
    );
}

#[test]
fn test_moon_check_raw_output() {
    let dir = TestDir::new("warns/deny_warn");

    check(
        get_stdout(&dir, ["check", "--no-render", "--sort-input", "-j1", "-q"]),
        expect![[r#"
            $ROOT/lib/hello.mbt:4:7-4:8 [E0002] Warning (unused_value): Unused variable 'a'
            $ROOT/lib/hello.mbt:11:7-11:9 [E0002] Warning (unused_value): Unused variable '中文'
            $ROOT/lib/hello.mbt:12:7-12:12 [E0002] Warning (unused_value): Unused variable '🤣😭🤣😭🤣'
            $ROOT/main/main.mbt:2:7-2:8 [E0002] Warning (unused_value): Unused variable 'a'
        "#]],
    );
}
