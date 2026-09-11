use crate::{TestDir, build_graph, moon_cmd};
use expect_test::expect_file;

#[track_caller]
fn assert_dry_run_graph(dir: &TestDir, args: &[&str], expected: expect_test::ExpectFile) {
    build_graph::assert_with_replacements(
        moon_cmd(dir).args(args).env("MOONBIT_NEW_NATIVE", "0"),
        expected,
        |s| {
            // Normalize clang-only warnings to keep snapshots portable across macOS/Linux.
            *s = s.replace(" -Wno-unused-value", "");
            *s = s.replace(".dylib", ".so");
            crate::util::normalize_host_archiver(s);
        },
    );
}

#[test]
fn test_use_cc_for_native_release() {
    let dir = TestDir::new("moon_test/hello_exec_fntest");
    // build
    {
        assert_dry_run_graph(
            &dir,
            &[
                "build",
                "--target",
                "native",
                "--release",
                "--sort-input",
                "--dry-run",
            ],
            expect_file!["cc_for_native_release/build_release_graph.jsonl.snap"],
        );
        // Keep a debug-profile baseline for the generated-C backend.
        assert_dry_run_graph(
            &dir,
            &["build", "--target", "native", "--sort-input", "--dry-run"],
            expect_file!["cc_for_native_release/build_graph.jsonl.snap"],
        );
        assert_dry_run_graph(
            &dir,
            &[
                "build",
                "--target",
                "native",
                "--debug",
                "--sort-input",
                "--dry-run",
            ],
            expect_file!["cc_for_native_release/build_debug_graph.jsonl.snap"],
        );
    }

    // run
    {
        assert_dry_run_graph(
            &dir,
            &[
                "run",
                "main",
                "--target",
                "native",
                "--release",
                "--sort-input",
                "--dry-run",
            ],
            expect_file!["cc_for_native_release/run_release_graph.jsonl.snap"],
        );
        // Keep a debug-profile baseline for the generated-C backend.
        assert_dry_run_graph(
            &dir,
            &[
                "run",
                "main",
                "--target",
                "native",
                "--sort-input",
                "--dry-run",
            ],
            expect_file!["cc_for_native_release/run_graph.jsonl.snap"],
        );
        assert_dry_run_graph(
            &dir,
            &[
                "run",
                "main",
                "--target",
                "native",
                "--debug",
                "--sort-input",
                "--dry-run",
            ],
            expect_file!["cc_for_native_release/run_debug_graph.jsonl.snap"],
        );
    }

    // test
    {
        assert_dry_run_graph(
            &dir,
            &[
                "test",
                "--target",
                "native",
                "--release",
                "--sort-input",
                "--dry-run",
            ],
            expect_file!["cc_for_native_release/test_release_graph.jsonl.snap"],
        );
    }
}
