use crate::{TestDir, get_stdout, moon_cmd, util::check};
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

#[test]
fn outline_skips_native_compilation() {
    let dir = TestDir::new_empty();
    std::fs::write(dir.join("moon.mod.json"), r#"{"name":"test/outline"}"#).unwrap();
    std::fs::write(dir.join("moon.pkg.json"), r#"{"native-stub":["broken.c"]}"#).unwrap();
    std::fs::write(
        dir.join("broken.c"),
        "#error outline must not compile C stubs\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("test.mbt"),
        "test \"alpha\" { inspect(1, content=\"1\") }\n",
    )
    .unwrap();

    moon_cmd(&dir)
        .args(["test", "--outline", "--target", "native", "--quiet"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
   1. test/outline test.mbt:1 index=0 name="alpha"

"#]]);

    // Metadata generation may also emit the driver source, but it must not
    // compile MoonBit packages or produce native code and link artifacts.
    for entry in walkdir::WalkDir::new(dir.join("_build")) {
        let entry = entry.unwrap();
        assert!(
            !matches!(
                entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str()),
                Some("core" | "mi" | "c" | "o" | "obj" | "exe")
            ),
            "outline produced a compiled artifact: {}",
            entry.path().display()
        );
    }
}

#[test]
fn standalone_outline_only_needs_test_metadata() {
    let dir = TestDir::new_empty();
    std::fs::write(
        dir.join("script.mbtx"),
        "test \"alpha\" { missing_function() }\n",
    )
    .unwrap();

    moon_cmd(&dir)
        .args([
            "test",
            "script.mbtx",
            "--outline",
            "--target",
            "js",
            "--quiet",
        ])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
   1. moon/test/single script.mbtx:1 index=0 name="alpha"

"#]]);
}
