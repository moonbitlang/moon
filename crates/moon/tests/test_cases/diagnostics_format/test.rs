use expect_test::expect;

use crate::{TestDir, get_stderr, get_stdout, util::check};

#[test]
fn test_moon_test_json_output() {
    let dir = TestDir::new("warns/deny_warn");

    check(
        get_stdout(&dir, ["test", "--output-json", "--sort-input", "-j1", "-q"]),
        expect![[r#"
            {"$message_type":"diagnostic","level":"warning","error_code":2,"path":"$ROOT/lib/hello.mbt","loc":"4:7-4:8","message":"Warning (unused_value): Unused variable 'a'"}
            {"$message_type":"diagnostic","level":"warning","error_code":2,"path":"$ROOT/lib/hello.mbt","loc":"11:7-11:9","message":"Warning (unused_value): Unused variable '中文'"}
            {"$message_type":"diagnostic","level":"warning","error_code":2,"path":"$ROOT/lib/hello.mbt","loc":"12:7-12:12","message":"Warning (unused_value): Unused variable '🤣😭🤣😭🤣'"}
            {"$message_type":"diagnostic","level":"warning","error_code":2,"path":"$ROOT/main/main.mbt","loc":"2:7-2:8","message":"Warning (unused_value): Unused variable 'a'"}
        "#]],
    );
}

#[test]
fn test_moon_test_rendered_output() {
    let dir = TestDir::new("warns/deny_warn");

    check(
        get_stderr(&dir, ["test", "--sort-input", "-j1", "-q"]),
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
            Warning: no test entry found.
        "#]],
    );
}

#[test]
fn test_moon_test_raw_output() {
    let dir = TestDir::new("warns/deny_warn");

    check(
        get_stdout(&dir, ["test", "--no-render", "--sort-input", "-j1", "-q"]),
        expect![[r#"
            $ROOT/lib/hello.mbt:4:7-4:8 [E0002] Warning (unused_value): Unused variable 'a'
            $ROOT/lib/hello.mbt:11:7-11:9 [E0002] Warning (unused_value): Unused variable '中文'
            $ROOT/lib/hello.mbt:12:7-12:12 [E0002] Warning (unused_value): Unused variable '🤣😭🤣😭🤣'
            $ROOT/lib/hello.mbt:4:7-4:8 [E0002] Warning (unused_value): Unused variable 'a'
            $ROOT/lib/hello.mbt:11:7-11:9 [E0002] Warning (unused_value): Unused variable '中文'
            $ROOT/lib/hello.mbt:12:7-12:12 [E0002] Warning (unused_value): Unused variable '🤣😭🤣😭🤣'
            $ROOT/main/main.mbt:2:7-2:8 [E0002] Warning (unused_value): Unused variable 'a'
            $ROOT/main/main.mbt:2:7-2:8 [E0002] Warning (unused_value): Unused variable 'a'
        "#]],
    );
}
