use crate::build_graph::compare_graphs;

use super::*;
use expect_test::expect_file;
use moonbuild_debug::graph::ENV_VAR;

#[test]
fn dummy_core_writes_packages_json_for_selected_target() {
    let test_dir = TestDir::new("dummy_core");
    let dir = dunce::canonicalize(test_dir.as_ref()).unwrap();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check", "--sort-input"])
        .assert()
        .success();

    #[cfg(unix)]
    {
        let p = dir.join("_build/packages.json");
        expect_file!["./packages_wasm_gc.json.snap"]
            .assert_eq(&replace_dir(&std::fs::read_to_string(p).unwrap(), &dir))
    }
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check", "--target", "js", "--sort-input"])
        .assert()
        .success();

    #[cfg(unix)]
    {
        let p = dir.join("_build/packages.json");
        expect_file!["./packages_js.json.snap"]
            .assert_eq(&replace_dir(&std::fs::read_to_string(p).unwrap(), &dir))
    };
}

#[test]
fn dummy_core_bundle_dry_run_matches_snapshots() {
    let test_dir = TestDir::new("dummy_core");
    let dir = dunce::canonicalize(test_dir.as_ref()).unwrap();

    let test_coverage_dry_run_dump_file = test_dir.join("test_coverage.jsonl");
    get_stdout_with_envs(
        &dir,
        ["test", "--dry-run", "--enable-coverage", "--sort-input"],
        [(ENV_VAR, &test_coverage_dry_run_dump_file)],
    );
    compare_graphs(
        &test_coverage_dry_run_dump_file,
        expect_file!["./coverage.jsonl.snap"],
    );

    let bundle_dry_run_dump_file = test_dir.join("bundle_dry_run.jsonl");
    get_stdout_with_envs(
        &dir,
        ["bundle", "--dry-run", "--sort-input"],
        [(ENV_VAR, &bundle_dry_run_dump_file)],
    );
    compare_graphs(
        &bundle_dry_run_dump_file,
        expect_file!["./bundle.jsonl.snap"],
    );

    let wasm_bundle_dry_run_dump_file = test_dir.join("bundle_wasm_dry_run.jsonl");
    get_stdout_with_envs(
        &dir,
        ["bundle", "--dry-run", "--target", "wasm", "--sort-input"],
        [(ENV_VAR, &wasm_bundle_dry_run_dump_file)],
    );
    compare_graphs(
        &wasm_bundle_dry_run_dump_file,
        expect_file!["./bundle_wasm.jsonl.snap"],
    );

    let wasm_gc_bundle_dry_run_dump_file = test_dir.join("bundle_wasm_gc_dry_run.jsonl");
    get_stdout_with_envs(
        &dir,
        ["bundle", "--dry-run", "--target", "wasm-gc", "--sort-input"],
        [(ENV_VAR, &wasm_gc_bundle_dry_run_dump_file)],
    );
    compare_graphs(
        &wasm_gc_bundle_dry_run_dump_file,
        expect_file!["./bundle_wasm_gc.jsonl.snap"],
    );

    let js_bundle_dry_run_dump_file = test_dir.join("bundle_js_dry_run.jsonl");
    get_stdout_with_envs(
        &dir,
        ["bundle", "--dry-run", "--target", "js", "--sort-input"],
        [(ENV_VAR, &js_bundle_dry_run_dump_file)],
    );
    compare_graphs(
        &js_bundle_dry_run_dump_file,
        expect_file!["./bundle_js.jsonl.snap"],
    );

    let all_targets_bundle_dry_run_dump_file = test_dir.join("bundle_all_targets_dry_run.jsonl");
    get_stdout_with_envs(
        &dir,
        ["bundle", "--target", "all", "--dry-run", "--sort-input"],
        [(ENV_VAR, &all_targets_bundle_dry_run_dump_file)],
    );
    compare_graphs(
        &all_targets_bundle_dry_run_dump_file,
        expect_file!["./bundle_all_targets.jsonl.snap"],
    );
}
