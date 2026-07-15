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

use super::fixture::{
    PlanningFixture, parse_run_command, parse_test_command, planned_graph_inputs,
};

#[test]
fn command_string_run_parses_as_expected() {
    let (cli, cmd) = parse_run_command(&["run", "-c", r#"fn main { println("hello") }"#]);

    expect![[r#"
        (
            UniversalFlags {
                source_tgt_dir: SourceTargetDirs {
                    cwd: None,
                    target_dir: None,
                },
                workspace_env: Auto,
                quiet: false,
                verbose: false,
                trace: false,
                dry_run: false,
                build_graph: false,
                unstable_feature: FeatureGate {
                    rr_export_module_graph: false,
                    rr_export_package_graph: false,
                    rr_export_build_plan: false,
                    rr_n2_explain: false,
                    rr_moon_mod: true,
                    rr_moon_pkg: true,
                    wasi_link: true,
                },
            },
            RunSubcommand {
                package_or_mbt_file: None,
                command: Some(
                    "fn main { println(\"hello\") }",
                ),
                build_flags: BuildFlags {
                    std: false,
                    no_std: false,
                    debug: false,
                    release: false,
                    strip: false,
                    no_strip: false,
                    target: [],
                    serial: false,
                    enable_coverage: false,
                    sort_input: false,
                    output_wat: false,
                    deny_warn: false,
                    no_render: false,
                    output_json: false,
                    warn_list: None,
                    enable_value_tracing: false,
                    jobs: None,
                    render_no_loc: Error,
                    diagnostic_limit: None,
                },
                args: [],
                moonrun_policy: None,
                auto_sync_flags: AutoSyncFlags {
                    frozen: false,
                },
                build_only: false,
                profile: false,
            },
        )
    "#]]
    .assert_debug_eq(&(cli, cmd));
}

#[test]
fn command_string_run_short_alias_e_still_parses() {
    let (_, cmd) = parse_run_command(&["run", "-e", r#"fn main { println("hello") }"#]);

    assert_eq!(
        cmd.command.as_deref(),
        Some(r#"fn main { println("hello") }"#)
    );
    assert_eq!(cmd.package_or_mbt_file, None);
}

#[test]
fn run_diagnostic_limit_parses() {
    let (_, cmd) = parse_run_command(&[
        "run",
        "--diagnostic-limit",
        "10",
        "-e",
        r#"fn main { println("hello") }"#,
    ]);

    assert_eq!(cmd.build_flags.diagnostic_limit, Some(10));
}

#[test]
fn native_target_dry_run_test_command_parses_as_expected() {
    let (cli, cmd) =
        parse_test_command(&["test", "--target", "native", "--dry-run", "--sort-input"]);

    expect![[r#"
        (
            UniversalFlags {
                source_tgt_dir: SourceTargetDirs {
                    cwd: None,
                    target_dir: None,
                },
                workspace_env: Auto,
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
                    rr_moon_mod: true,
                    rr_moon_pkg: true,
                    wasi_link: true,
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
                    diagnostic_limit: None,
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
                profile: false,
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
fn test_diagnostic_limit_parses_without_update() {
    let (_, cmd) = parse_test_command(&["test", "--diagnostic-limit", "10"]);

    assert_eq!(cmd.build_flags.diagnostic_limit, Some(10));
    assert!(!cmd.update);
}

#[test]
fn test_update_limit_still_parses_with_update() {
    let (_, cmd) = parse_test_command(&["test", "--update", "--limit", "10"]);

    assert_eq!(cmd.limit, 10);
    assert_eq!(cmd.build_flags.diagnostic_limit, None);
    assert!(cmd.update);
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
    let inputs = planned_graph_inputs(&graph);

    assert!(inputs.contains("./server/server_wbtest.mbt"));
    assert!(inputs.contains("./deps/nativedep/lib/lib.mbt"));
    assert!(!inputs.contains("./web/web_wbtest.mbt"));
    assert!(!inputs.contains("./deps/jsdep/lib/lib.mbt"));
}
