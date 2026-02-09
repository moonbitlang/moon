use super::*;

fn normalize_test_output(output: &str) -> String {
    let parts: Vec<&str> = output.split("\n\n").collect();

    // Last part is usually the summary "Total tests: ..."
    let (blocks, summary) = if parts.last().is_some_and(|p| p.contains("Total tests:")) {
        (&parts[..parts.len() - 1], parts.last().unwrap())
    } else {
        (&parts[..], &"")
    };

    // Sort the test failure blocks
    let mut sorted_blocks: Vec<&str> = blocks.to_vec();
    sorted_blocks.sort();

    // Reconstruct
    let mut result = sorted_blocks.join("\n\n");
    if !summary.is_empty() {
        result.push_str("\n\n");
        result.push_str(summary);
    }

    result
}

#[test]
fn test_snapshot_test_target_js() {
    let dir = TestDir::new("snapshot_testing.in");
    let output = get_err_stdout(
        &dir,
        ["test", "--target", "js", "--sort-input", "--no-parallelize"],
    );
    let normalized_output = normalize_test_output(&output);
    check(
        &normalized_output,
        expect![[r#"
            [username/hello] test lib/hello.mbt:10 ("test snapshot 1") failed
            expect test failed at $ROOT/src/lib/hello.mbt:14:3
            Diff: (- expected, + actual)
            ----
            +hello
            +snapshot
            +testing
            ----

            [username/hello] test lib/hello.mbt:17 ("test inspect 2") failed
            expect test failed at $ROOT/src/lib/hello.mbt:18:3-18:15
            Diff: (- expected, + actual)
            ----
            +c
            ----

            [username/hello] test lib/hello.mbt:22 ("test snapshot 2") failed
            expect test failed at $ROOT/src/lib/hello.mbt:26:3
            Diff: (- expected, + actual)
            ----
            +should
            +be
            +work
            ----

            [username/hello] test lib/hello.mbt:5 ("test inspect 1") failed
            expect test failed at $ROOT/src/lib/hello.mbt:6:3-6:15
            Diff: (- expected, + actual)
            ----
            +a
            ----

            [username/hello] test lib/hello_test.mbt:7 ("snapshot in blackbox test") failed
            expect test failed at $ROOT/src/lib/hello_test.mbt:9:3
            Diff: (- expected, + actual)
            ----
            +Hello, world!
            ----

            Total tests: 6, passed: 1, failed: 5.
        "#]],
    );
    // I'm not sure whether `moon test` should generate `package.json` in the _build directory
    // assert!(dir.join("_build/js/debug/test/package.json").exists());
    let output = get_stdout(
        &dir,
        [
            "test",
            "--target",
            "js",
            "-u",
            "--sort-input",
            "--no-parallelize",
        ],
    );

    assert!(output.contains("Total tests: 6, passed: 6, failed: 0."));

    check(
        read(dir.join("src/lib/hello.mbt")),
        expect![[r#"
            pub fn hello() -> String {
              "Hello, world!"
            }

            test "test inspect 1" {
              inspect("a", content=(#|a
              ))
              inspect("b", content=(#|b
              ))
            }

            test "test snapshot 1" (it : @test.Test) {
              it.writeln("hello")
              it.writeln("snapshot")
              it.writeln("testing")
              it.snapshot!(filename="001.txt")
            }

            test "test inspect 2" {
              inspect("c", content=(#|c
              ))
              inspect("d", content=(#|d
              ))
            }

            test "test snapshot 2" (it : @test.Test) {
              it.writeln("should")
              it.writeln("be")
              it.writeln("work")
              it.snapshot!(filename="002.txt")
            }
        "#]],
    );
    check(
        read(dir.join("src/lib/__snapshot__/001.txt")),
        expect![[r#"
        hello
        snapshot
        testing
    "#]],
    );
    check(
        read(dir.join("src/lib/__snapshot__/002.txt")),
        expect![[r#"
        should
        be
        work
    "#]],
    );
    check(
        read(dir.join("src/lib/__snapshot__/003.txt")),
        expect!["Hello, world!"],
    );
}

#[test]
fn test_snapshot_test() {
    let dir = TestDir::new("snapshot_testing.in");
    let output = get_err_stdout(&dir, ["test", "--sort-input", "--no-parallelize"]);
    let normalized_output = normalize_test_output(&output);
    check(
        &normalized_output,
        expect![[r#"
            [username/hello] test lib/hello.mbt:10 ("test snapshot 1") failed
            expect test failed at $ROOT/src/lib/hello.mbt:14:3
            Diff: (- expected, + actual)
            ----
            +hello
            +snapshot
            +testing
            ----

            [username/hello] test lib/hello.mbt:17 ("test inspect 2") failed
            expect test failed at $ROOT/src/lib/hello.mbt:18:3-18:15
            Diff: (- expected, + actual)
            ----
            +c
            ----

            [username/hello] test lib/hello.mbt:22 ("test snapshot 2") failed
            expect test failed at $ROOT/src/lib/hello.mbt:26:3
            Diff: (- expected, + actual)
            ----
            +should
            +be
            +work
            ----

            [username/hello] test lib/hello.mbt:5 ("test inspect 1") failed
            expect test failed at $ROOT/src/lib/hello.mbt:6:3-6:15
            Diff: (- expected, + actual)
            ----
            +a
            ----

            [username/hello] test lib/hello_test.mbt:7 ("snapshot in blackbox test") failed
            expect test failed at $ROOT/src/lib/hello_test.mbt:9:3
            Diff: (- expected, + actual)
            ----
            +Hello, world!
            ----

            Total tests: 6, passed: 1, failed: 5.
        "#]],
    );

    let output_native = get_err_stdout(
        &dir,
        [
            "test",
            "--sort-input",
            "--no-parallelize",
            "--target",
            "native",
        ],
    );
    let normalized_output_native = normalize_test_output(&output_native);
    check(
        &normalized_output_native,
        expect![[r#"
            [username/hello] test lib/hello.mbt:10 ("test snapshot 1") failed
            expect test failed at $ROOT/src/lib/hello.mbt:14:3
            Diff: (- expected, + actual)
            ----
            +hello
            +snapshot
            +testing
            ----

            [username/hello] test lib/hello.mbt:17 ("test inspect 2") failed
            expect test failed at $ROOT/src/lib/hello.mbt:18:3-18:15
            Diff: (- expected, + actual)
            ----
            +c
            ----

            [username/hello] test lib/hello.mbt:22 ("test snapshot 2") failed
            expect test failed at $ROOT/src/lib/hello.mbt:26:3
            Diff: (- expected, + actual)
            ----
            +should
            +be
            +work
            ----

            [username/hello] test lib/hello.mbt:5 ("test inspect 1") failed
            expect test failed at $ROOT/src/lib/hello.mbt:6:3-6:15
            Diff: (- expected, + actual)
            ----
            +a
            ----

            [username/hello] test lib/hello_test.mbt:7 ("snapshot in blackbox test") failed
            expect test failed at $ROOT/src/lib/hello_test.mbt:9:3
            Diff: (- expected, + actual)
            ----
            +Hello, world!
            ----

            Total tests: 6, passed: 1, failed: 5.
        "#]],
    );

    let update_output = get_stdout(&dir, ["test", "-u", "--no-parallelize"]);
    assert!(update_output.contains("Total tests: 6, passed: 6, failed: 0."));

    check(
        read(dir.join("src/lib/hello.mbt")),
        expect![[r#"
            pub fn hello() -> String {
              "Hello, world!"
            }

            test "test inspect 1" {
              inspect("a", content=(#|a
              ))
              inspect("b", content=(#|b
              ))
            }

            test "test snapshot 1" (it : @test.Test) {
              it.writeln("hello")
              it.writeln("snapshot")
              it.writeln("testing")
              it.snapshot!(filename="001.txt")
            }

            test "test inspect 2" {
              inspect("c", content=(#|c
              ))
              inspect("d", content=(#|d
              ))
            }

            test "test snapshot 2" (it : @test.Test) {
              it.writeln("should")
              it.writeln("be")
              it.writeln("work")
              it.snapshot!(filename="002.txt")
            }
        "#]],
    );
    check(
        read(dir.join("src/lib/__snapshot__/001.txt")),
        expect![[r#"
        hello
        snapshot
        testing
    "#]],
    );
    check(
        read(dir.join("src/lib/__snapshot__/002.txt")),
        expect![[r#"
        should
        be
        work
    "#]],
    );
    check(
        read(dir.join("src/lib/__snapshot__/003.txt")),
        expect!["Hello, world!"],
    );
}
