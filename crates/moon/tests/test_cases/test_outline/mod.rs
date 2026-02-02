use crate::{TestDir, get_stdout, util::check};
use expect_test::expect;

fn normalize_outline(output: String) -> String {
    let mut out = output
        .lines()
        .map(|line| line.trim_start())
        .collect::<Vec<_>>()
        .join("\n");
    out.push('\n');
    out
}

#[test]
fn test_outline() {
    let dir = TestDir::new("test_outline.in");
    let output = normalize_outline(get_stdout(&dir, ["test", "--outline", "-q"]));
    check(
        output,
        expect![[r#"
1. username/outline/lib hello.mbt:1 index=0 name="alpha"
2. username/outline/lib hello.mbt:5 index=1
3. username/outline/lib hello.mbt:9 index=2 name="beta"
"#]],
    );

    let output = normalize_outline(get_stdout(&dir, ["test", "--outline", "-q", "-F", "b*"]));
    check(
        output,
        expect![[r#"
1. username/outline/lib hello.mbt:9 index=2 name="beta"
"#]],
    );
}
