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

use expect_test::expect_file;

use super::fixture::{PlanningFixture, parse_test_command};

// Phase 3: with the package filter already chosen, planner tests only need to
// assert the resulting dry-run graph.

#[test]
fn filtered_package_graph_matches_snapshot() {
    let fixture = PlanningFixture::new("test_filter/test_filter").expect("fixture should resolve");
    let (cli, cmd) = parse_test_command(&[
        "test",
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
    let (cli, cmd) = parse_test_command(&["test", "--sort-input", "--dry-run"]);

    expect_file!["./snapshots/package_filter_workspace_graph.jsonl.snap"].assert_eq(
        &fixture
            .plan_test_with_cli(&cli, &cmd)
            .expect("planner should build the workspace test graph"),
    );
}
