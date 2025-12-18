use super::*;

#[test]
fn test_dedup_diag() {
    let dir = TestDir::new("dedup_diag.in");
    let out = get_stdout(&dir, ["test", "--output-json"]);

    check(out, expect![[r#"
        {"$message_type":"diagnostic","level":"warning","loc":{"path":"$ROOT/test.mbt","start":{"line":3,"col":7},"end":{"line":3,"col":8}},"message":"Warning (unused_value): Unused variable 'a'","error_code":2}
        Total tests: 1, passed: 1, failed: 0.
    "#]])
}
