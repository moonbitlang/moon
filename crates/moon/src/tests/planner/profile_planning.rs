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

use super::fixture::{PlanningFixture, parse_bench_command, parse_test_command};

// Phase 3: these tests start from an already chosen command configuration and
// assert how the planner lowers that intent into the dry-run graph.

#[test]
fn bench_graph_uses_selected_codegen_profile() {
    let fixture = PlanningFixture::new("moon_bench").expect("fixture should resolve");
    let (cli, cmd) = parse_bench_command(&["bench", "--sort-input", "--dry-run"]);

    let default_graph = fixture
        .plan_bench_with_cli(&cli, &cmd)
        .expect("default bench graph should plan");
    assert!(default_graph.contains("moonc"));
    assert!(!default_graph.contains("-O0"));

    let (release_cli, release_cmd) =
        parse_bench_command(&["bench", "--release", "--sort-input", "--dry-run"]);
    let release_graph = fixture
        .plan_bench_with_cli(&release_cli, &release_cmd)
        .expect("release bench graph should plan");
    assert!(release_graph.contains("moonc"));
    assert!(!release_graph.contains("-O0"));

    let (debug_cli, debug_cmd) =
        parse_bench_command(&["bench", "--debug", "--sort-input", "--dry-run"]);
    let debug_graph = fixture
        .plan_bench_with_cli(&debug_cli, &debug_cmd)
        .expect("debug bench graph should plan");
    assert!(debug_graph.contains("moonc"));
    assert!(debug_graph.contains("-O0"));
}

#[test]
fn default_test_graph_matches_snapshot() {
    let fixture = PlanningFixture::new("test_release").expect("fixture should resolve");
    let (cli, cmd) = parse_test_command(&["test", "--sort-input", "--dry-run"]);

    expect_file!["./snapshots/test_profile_default_graph.jsonl.snap"].assert_eq(
        &fixture
            .plan_test_with_cli(&cli, &cmd)
            .expect("default test graph should plan"),
    );
}

#[test]
fn release_test_graph_matches_snapshot() {
    let fixture = PlanningFixture::new("test_release").expect("fixture should resolve");
    let (cli, cmd) = parse_test_command(&["test", "--release", "--sort-input", "--dry-run"]);

    expect_file!["./snapshots/test_profile_release_graph.jsonl.snap"].assert_eq(
        &fixture
            .plan_test_with_cli(&cli, &cmd)
            .expect("release test graph should plan"),
    );
}
