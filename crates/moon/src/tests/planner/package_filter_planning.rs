// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use expect_test::expect_file;

use super::fixture::{PlanningFixture, parse_test_command};

// Phase 3: with the package filter already chosen, planner tests only need to
// assert the resulting dry-run graph.

#[test]
fn filtered_package_graph_matches_snapshot() {
    let fixture = PlanningFixture::new("test_filter/test_filter").expect("fixture should resolve");
    let (cli, cmd) = parse_test_command(&[
        "test",
        "--target",
        "wasm-gc",
        "-p",
        "username/hello/A",
        "--sort-input",
        "--dry-run",
    ]);

    expect_file!["./snapshots/package_filter_filtered_graph.jsonl.snap"].assert_eq(
        &fixture
            .plan_test_with_cli(&cli, &cmd)
            .expect("planner should build the filtered test graph"),
    );
}

#[test]
fn workspace_test_graph_matches_snapshot() {
    let fixture = PlanningFixture::new("test_filter/test_filter").expect("fixture should resolve");
    let (cli, cmd) =
        parse_test_command(&["test", "--target", "wasm-gc", "--sort-input", "--dry-run"]);

    expect_file!["./snapshots/package_filter_workspace_graph.jsonl.snap"].assert_eq(
        &fixture
            .plan_test_with_cli(&cli, &cmd)
            .expect("planner should build the workspace test graph"),
    );
}
