use super::*;

#[test]
fn test_dep_order() {
    let dir = TestDir::new("dep_order/dep_order");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
      1
    "#]],
    )
}
