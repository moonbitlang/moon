use crate::build_graph::compare_graphs;

use super::*;
use expect_test::expect_file;
use moonbuild_debug::graph::ENV_VAR;

fn check_command_source_summary(dir: &std::path::Path, target: &str) -> String {
    moon_cmd(&dir)
        .args(["check", "--target", target, "--sort-input"])
        .assert()
        .success();

    let packages: moonutil::manifest::ModuleDBJSON =
        serde_json::from_str(&std::fs::read_to_string(dir.join("_build/packages.json")).unwrap())
            .unwrap();
    let mut lines = vec![format!("target: {}", packages.backend)];

    for package in packages.packages {
        for (kind, command) in [
            ("check", package.check_command.as_deref()),
            ("whitebox", package.wbtest_check_command.as_deref()),
            ("blackbox", package.test_check_command.as_deref()),
        ] {
            let Some(command) = command else {
                continue;
            };
            let sources = command
                .iter()
                .filter(|argument| argument.ends_with(".mbt"))
                .map(|source| {
                    std::path::Path::new(source)
                        .strip_prefix(&package.root_path)
                        .expect("check command source should be inside its package")
                        .to_string_lossy()
                        .replace('\\', "/")
                })
                .collect::<Vec<_>>();
            if !sources.is_empty() {
                lines.push(format!(
                    "package {} {kind}: {}",
                    package.rel,
                    sources.join(", ")
                ));
            }
        }
    }

    lines.join("\n")
}

#[test]
fn dummy_core_selects_target_sources_for_check_commands() {
    let test_dir = TestDir::new("dummy_core");
    let dir = dunce::canonicalize(test_dir.as_ref()).unwrap();

    snapbox::assert_data_eq!(
        check_command_source_summary(&dir, "wasm-gc"),
        snapbox::str![[r#"
target: wasm-gc
package 0 check: lib.mbt, y.wasm-gc.mbt
package 0 whitebox: lib.mbt, y.wasm-gc.mbt, y_wbtest.mbt, y_wbtest.wasm-gc.mbt
package 0 blackbox: lib.mbt, y.wasm-gc.mbt
package 1 check: lib.mbt, x.wasm-gc.mbt
package 1 whitebox: lib.mbt, x.wasm-gc.mbt, x_wbtest.wasm-gc.mbt
package 1 blackbox: lib.mbt, x.wasm-gc.mbt
package 2 check: lib.mbt
package 2 blackbox: lib.mbt
"#]],
    );
    snapbox::assert_data_eq!(
        check_command_source_summary(&dir, "js"),
        snapbox::str![[r#"
target: js
package 0 check: lib.mbt, y.js.mbt
package 0 whitebox: lib.mbt, y.js.mbt, y_wbtest.js.mbt, y_wbtest.mbt
package 0 blackbox: lib.mbt, y.js.mbt
package 1 check: lib.mbt, x.js.mbt
package 1 whitebox: lib.mbt, x.js.mbt
package 1 blackbox: lib.mbt, x.js.mbt
package 2 check: lib.mbt
package 2 blackbox: lib.mbt
"#]],
    );
}

#[test]
fn dummy_core_bundle_dry_run_matches_snapshots() {
    let test_dir = TestDir::new("dummy_core");
    let dir = dunce::canonicalize(test_dir.as_ref()).unwrap();

    let test_coverage_dry_run_dump_file = test_dir.join("test_coverage.jsonl");
    get_stdout_with_envs(
        &dir,
        [
            "test",
            "--target",
            "wasm-gc",
            "--dry-run",
            "--enable-coverage",
            "--sort-input",
        ],
        [(ENV_VAR, &test_coverage_dry_run_dump_file)],
    );
    compare_graphs(
        &test_coverage_dry_run_dump_file,
        expect_file!["./coverage.jsonl.snap"],
    );

    let bundle_dry_run_dump_file = test_dir.join("bundle_dry_run.jsonl");
    get_stdout_with_envs(
        &dir,
        ["bundle", "--target", "wasm-gc", "--dry-run", "--sort-input"],
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
