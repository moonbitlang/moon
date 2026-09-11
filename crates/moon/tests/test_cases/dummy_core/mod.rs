use super::*;
use expect_test::expect_file;

#[test]
fn dummy_core_writes_packages_json_for_selected_target() {
    let test_dir = TestDir::new("dummy_core");
    let dir = dunce::canonicalize(test_dir.as_ref()).unwrap();

    moon_cmd(&dir)
        .args(["check", "--target", "wasm-gc", "--sort-input"])
        .assert()
        .success();

    #[cfg(unix)]
    {
        let p = scoped_packages_json_path(&dir, "wasm-gc", "debug");
        expect_file!["./packages_wasm_gc.json.snap"]
            .assert_eq(&replace_dir(&std::fs::read_to_string(p).unwrap(), &dir))
    }
    moon_cmd(&dir)
        .args(["check", "--target", "js", "--sort-input"])
        .assert()
        .success();

    #[cfg(unix)]
    {
        let p = scoped_packages_json_path(&dir, "js", "debug");
        expect_file!["./packages_js.json.snap"]
            .assert_eq(&replace_dir(&std::fs::read_to_string(p).unwrap(), &dir))
    };
}

#[test]
fn dummy_core_bundle_dry_run_matches_snapshots() {
    let test_dir = TestDir::new("dummy_core");
    let dir = dunce::canonicalize(test_dir.as_ref()).unwrap();

    build_graph::assert(
        moon_cmd(&dir).args([
            "test",
            "--target",
            "wasm-gc",
            "--dry-run",
            "--enable-coverage",
            "--sort-input",
        ]),
        expect_file!["./coverage.jsonl.snap"],
    );

    build_graph::assert(
        moon_cmd(&dir).args(["bundle", "--target", "wasm-gc", "--dry-run", "--sort-input"]),
        expect_file!["./bundle.jsonl.snap"],
    );

    build_graph::assert(
        moon_cmd(&dir).args(["bundle", "--dry-run", "--target", "wasm", "--sort-input"]),
        expect_file!["./bundle_wasm.jsonl.snap"],
    );

    build_graph::assert(
        moon_cmd(&dir).args(["bundle", "--dry-run", "--target", "wasm-gc", "--sort-input"]),
        expect_file!["./bundle_wasm_gc.jsonl.snap"],
    );

    build_graph::assert(
        moon_cmd(&dir).args(["bundle", "--dry-run", "--target", "js", "--sort-input"]),
        expect_file!["./bundle_js.jsonl.snap"],
    );

    build_graph::assert(
        moon_cmd(&dir).args(["bundle", "--target", "all", "--dry-run", "--sort-input"]),
        expect_file!["./bundle_all_targets.jsonl.snap"],
    );
}
