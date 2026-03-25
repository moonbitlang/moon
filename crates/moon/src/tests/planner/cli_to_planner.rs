// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use expect_test::expect;

use super::fixture::{PlanningFixture, parse_test_command};

#[test]
fn native_target_dry_run_test_command_parses_as_expected() {
    let (cli, cmd) =
        parse_test_command(&["test", "--target", "native", "--dry-run", "--sort-input"]);

    expect![[r#"
        (
            UniversalFlags {
                source_tgt_dir: SourceTargetDirs {
                    cwd: None,
                    manifest_path: None,
                    target_dir: None,
                },
                quiet: false,
                verbose: false,
                trace: false,
                dry_run: true,
                build_graph: false,
                unstable_feature: FeatureGate {
                    rr_export_module_graph: false,
                    rr_export_package_graph: false,
                    rr_export_build_plan: false,
                    rr_n2_explain: false,
                    rr_moon_pkg: true,
                },
            },
            TestSubcommand {
                build_flags: BuildFlags {
                    std: false,
                    no_std: false,
                    debug: false,
                    release: false,
                    strip: false,
                    no_strip: false,
                    target: [
                        Native,
                    ],
                    serial: false,
                    enable_coverage: false,
                    sort_input: true,
                    output_wat: false,
                    deny_warn: false,
                    no_render: false,
                    output_json: false,
                    warn_list: None,
                    enable_value_tracing: false,
                    jobs: None,
                    render_no_loc: Error,
                },
                package: None,
                file: None,
                index: None,
                doc_index: None,
                update: false,
                limit: 256,
                auto_sync_flags: AutoSyncFlags {
                    frozen: false,
                },
                build_only: false,
                no_parallelize: false,
                outline: false,
                test_failure_json: false,
                patch_file: None,
                doc_test: false,
                path: [],
                include_skipped: false,
                filter: None,
            },
        )
    "#]]
    .assert_debug_eq(&(cli, cmd));
}

#[test]
fn parsed_native_target_dry_run_test_command_plans_native_graph() {
    let (cli, cmd) =
        parse_test_command(&["test", "--target", "native", "--dry-run", "--sort-input"]);
    let fixture =
        PlanningFixture::new("mixed_backend_local_dep.in").expect("fixture should resolve");

    let graph = fixture
        .plan_test_with_cli(&cli, &cmd)
        .expect("native target test graph should plan");

    assert!(graph.contains("./server/server_wbtest.mbt"));
    assert!(graph.contains("./deps/nativedep/lib/lib.mbt"));
    assert!(!graph.contains("./web/web_wbtest.mbt"));
    assert!(!graph.contains("./deps/jsdep/lib/lib.mbt"));
}
