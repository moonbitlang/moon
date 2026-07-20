use expect_test::{expect, expect_file};

use crate::{TestDir, assert_dry_run_graph, get_stdout, util::check};

#[test]
fn test_moon_test_patch() {
    let dir = TestDir::new("moon_test/patch");

    // Apply patch to normal build
    assert_dry_run_graph(
        &dir,
        [
            "test",
            "--target",
            "wasm-gc",
            "-p",
            "moon_new/lib",
            "--file",
            "hello_0.mbt",
            "--patch-file",
            "./patch.json",
            "--dry-run",
            "--sort-input",
            "--nostd",
        ],
        expect_file!["patch_dry_run_graph.jsonl.snap"],
    );
    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "wasm-gc",
                "-p",
                "moon_new/lib",
                "--file",
                "hello_0.mbt",
                "--patch-file",
                "./patch.json",
            ],
        ),
        expect![[r#"
            hello from patch.json
            hello from lib/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    // Apply patch to white box test
    assert_dry_run_graph(
        &dir,
        [
            "test",
            "--target",
            "wasm-gc",
            "-p",
            "moon_new/lib",
            "--file",
            "hello_1_wbtest.mbt",
            "--patch-file",
            "./patch_wbtest.json",
            "--dry-run",
            "--sort-input",
            "--nostd",
        ],
        expect_file!["patch_wbtest_dry_run_graph.jsonl.snap"],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "wasm-gc",
                "-p",
                "moon_new/lib",
                "--file",
                "hello_1_wbtest.mbt",
                "--patch-file",
                "./patch_wbtest.json",
            ],
        ),
        expect![[r#"
            hello from patch_wbtest.json
            hello from lib/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    // Apply patch to black box test
    assert_dry_run_graph(
        &dir,
        [
            "test",
            "--target",
            "wasm-gc",
            "-p",
            "moon_new/lib",
            "--file",
            "hello_2_test.mbt",
            "--patch-file",
            "./patch_test.json",
            "--dry-run",
            "--sort-input",
            "--nostd",
        ],
        expect_file!["patch_bbtest_dry_run_graph.jsonl.snap"],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "wasm-gc",
                "-p",
                "moon_new/lib",
                "--file",
                "hello_2_test.mbt",
                "--patch-file",
                "./patch_test.json",
            ],
        ),
        expect![[r#"
            hello from patch_test.json
            hello from lib/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    // no _test.mbt and _wbtest.mbt in original package
    assert_dry_run_graph(
        &dir,
        [
            "test",
            "--target",
            "wasm-gc",
            "-p",
            "moon_new/lib2",
            "--file",
            "hello_2_test.mbt",
            "--patch-file",
            "./2.patch_test.json",
            "--dry-run",
            "--sort-input",
            "--nostd",
        ],
        expect_file!["patch_2_bbtest_dry_run_graph.jsonl.snap"],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "wasm-gc",
                "-p",
                "moon_new/lib2",
                "--file",
                "hello_2_test.mbt",
                "--patch-file",
                "./2.patch_test.json",
            ],
        ),
        expect![[r#"
            hello from 2.patch_test.json
            hello from lib2/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    assert_dry_run_graph(
        &dir,
        [
            "test",
            "--target",
            "wasm-gc",
            "-p",
            "moon_new/lib2",
            "--file",
            "hello_2_wbtest.mbt",
            "--patch-file",
            "./2.patch_wbtest.json",
            "--dry-run",
            "--sort-input",
            "--nostd",
        ],
        expect_file!["patch_2_wbtest_dry_run_graph.jsonl.snap"],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "wasm-gc",
                "-p",
                "moon_new/lib2",
                "--file",
                "hello_2_wbtest.mbt",
                "--patch-file",
                "./2.patch_wbtest.json",
            ],
        ),
        expect![[r#"
            hello from 2.patch_wbtest.json
            hello from lib2/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_patch_content_change_regenerates_driver() {
    let dir = TestDir::new("moon_test/patch");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "wasm-gc",
                "-p",
                "moon_new/lib2",
                "--file",
                "hello_2_test.mbt",
                "--patch-file",
                "./2.patch_test.json",
            ],
        ),
        expect![[r#"
            hello from 2.patch_test.json
            hello from lib2/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    // Rewrite the patch file with different injected content, keeping both the
    // patch file path and the injected file name unchanged. The test driver
    // must still be regenerated; otherwise stale test names are reported and
    // newly added tests silently never run.
    std::fs::write(
        dir.join("2.patch_test.json"),
        r#"{
  "drops": [],
  "patches": [
    {
      "name": "hello_2_test.mbt",
      "content": "test \"updated_first\" { \n println(\"updated first\") \n } \n test \"updated_second\" { \n println(\"updated second\") \n }"
    }
  ]
}"#,
    )
    .unwrap();

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "wasm-gc",
                "-p",
                "moon_new/lib2",
                "--file",
                "hello_2_test.mbt",
                "--patch-file",
                "./2.patch_test.json",
            ],
        ),
        expect![[r#"
            updated first
            updated second
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
}
