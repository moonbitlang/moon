use expect_test::expect;

use crate::{TestDir, get_stderr, get_stdout, util::check};

#[test]
fn test_moon_build_json_output() {
    let dir = TestDir::new("warns/deny_warn");

    check(
        get_stdout(
            &dir,
            ["build", "--output-json", "--sort-input", "-j1", "-q"],
        ),
        expect![[r#"
            {"$message_type":"diagnostic","level":"warning","loc":{"path":"$ROOT/lib/hello.mbt","start":{"line":4,"col":7},"end":{"line":4,"col":8}},"message":"Warning: Unused variable 'a'","error_code":2}
            {"$message_type":"diagnostic","level":"warning","loc":{"path":"$ROOT/lib/hello.mbt","start":{"line":11,"col":7},"end":{"line":11,"col":9}},"message":"Warning: Unused variable '中文'","error_code":2}
            {"$message_type":"diagnostic","level":"warning","loc":{"path":"$ROOT/lib/hello.mbt","start":{"line":12,"col":7},"end":{"line":12,"col":12}},"message":"Warning: Unused variable '🤣😭🤣😭🤣'","error_code":2}
            {"$message_type":"diagnostic","level":"warning","loc":{"path":"$ROOT/main/main.mbt","start":{"line":2,"col":7},"end":{"line":2,"col":8}},"message":"Warning: Unused variable 'a'","error_code":2}
        "#]],
    );
}

#[test]
fn test_moon_build_rendered_output() {
    let dir = TestDir::new("warns/deny_warn");

    check(
        get_stderr(&dir, ["build", "--sort-input", "-j1", "-q"]),
        expect![[r#"
            Warning: [0002]
               ╭─[ $ROOT/lib/hello.mbt:4:7 ]
               │
             4 │   let a = 1;
               │       ┬  
               │       ╰── Warning: Unused variable 'a'
            ───╯
            Warning: [0002]
                ╭─[ $ROOT/lib/hello.mbt:11:7 ]
                │
             11 │   let 中文 = 2
                │       ──┬─  
                │         ╰─── Warning: Unused variable '中文'
            ────╯
            Warning: [0002]
                ╭─[ $ROOT/lib/hello.mbt:12:7 ]
                │
             12 │   let 🤣😭🤣😭🤣 = 2
                │       ────┬─────  
                │           ╰─────── Warning: Unused variable '🤣😭🤣😭🤣'
            ────╯
            Warning: [0002]
               ╭─[ $ROOT/main/main.mbt:2:7 ]
               │
             2 │   let a = 0
               │       ┬  
               │       ╰── Warning: Unused variable 'a'
            ───╯
        "#]],
    );
}

#[test]
fn test_moon_build_raw_output() {
    let dir = TestDir::new("warns/deny_warn");

    check(
        get_stdout(&dir, ["build", "--no-render", "--sort-input", "-j1", "-q"]),
        expect![[r#"
            $ROOT/lib/hello.mbt:4:7-4:8 [E0002] Warning: Unused variable 'a'
            $ROOT/lib/hello.mbt:11:7-11:9 [E0002] Warning: Unused variable '中文'
            $ROOT/lib/hello.mbt:12:7-12:12 [E0002] Warning: Unused variable '🤣😭🤣😭🤣'
            $ROOT/main/main.mbt:2:7-2:8 [E0002] Warning: Unused variable 'a'
        "#]],
    );
}
