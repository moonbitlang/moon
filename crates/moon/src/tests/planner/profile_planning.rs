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

use super::fixture::{
    PlanningFixture, parse_bench_command, parse_build_command, parse_check_command,
    parse_run_command, parse_test_command,
};

// Phase 3: these tests start from an already chosen command configuration and
// assert how the planner lowers that intent into the dry-run graph.

fn line_with<'a>(graph: &'a str, command: &str, filter: &[&str]) -> &'a str {
    graph
        .lines()
        .find(|line| line.contains(command) && filter.iter().all(|needle| line.contains(needle)))
        .unwrap_or_else(|| {
            panic!(
                "expected graph line containing `{command}` and {:?}\n{graph}",
                filter
            )
        })
}

fn command_tokens(graph: &str, command: &str, filter: &[&str]) -> Vec<String> {
    let line = line_with(graph, command, filter);
    let line_json: serde_json::Value =
        serde_json::from_str(line).expect("planner dump line should be valid JSON");
    let command = line_json["command"]
        .as_str()
        .expect("planner dump line should contain a command");
    shlex::split(command).expect("planner command should tokenize")
}

fn assert_command_uses_profile(
    graph: &str,
    command: &str,
    filter: &[&str],
    profile: &str,
    expect_debug_flags: bool,
) {
    let tokens = command_tokens(graph, command, filter);
    let expected_prefix = format!("./_build/wasm-gc/{profile}/");
    let other_prefix = if profile == "debug" {
        "./_build/wasm-gc/release/"
    } else {
        "./_build/wasm-gc/debug/"
    };

    assert!(
        tokens
            .iter()
            .filter(|token| token.contains("./_build/wasm-gc/"))
            .all(|token| token.contains(&expected_prefix)),
        "expected `{command}` with {:?} to use `{expected_prefix}`, got:\n{:?}",
        filter,
        tokens,
    );
    assert!(
        tokens
            .iter()
            .filter(|token| token.contains("./_build/wasm-gc/"))
            .all(|token| !token.contains(other_prefix)),
        "expected `{command}` with {:?} to avoid `{other_prefix}`, got:\n{:?}",
        filter,
        tokens,
    );

    for flag in ["-g", "-O0", "-source-map"] {
        assert_eq!(
            tokens.iter().any(|token| token == flag),
            expect_debug_flags,
            "expected `{command}` with {:?} to {} `{flag}`, got:\n{:?}",
            filter,
            if expect_debug_flags {
                "include"
            } else {
                "omit"
            },
            tokens,
        );
    }
}

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

#[test]
fn check_graph_uses_debug_profile_without_codegen_flags() {
    let fixture = PlanningFixture::new("debug_flag_test").expect("fixture should resolve");

    for (label, args) in [
        (
            "default check",
            ["check", "--dry-run", "--nostd", "--sort-input"].as_slice(),
        ),
        (
            "explicit debug check",
            ["check", "--dry-run", "--debug", "--nostd", "--sort-input"].as_slice(),
        ),
    ] {
        let (cli, cmd) = parse_check_command(args);
        let graph = fixture
            .plan_check_with_cli(&cli, &cmd)
            .unwrap_or_else(|err| panic!("{label} should plan: {err:#}"));

        assert_command_uses_profile(&graph, "moonc check", &["./lib/hello.mbt"], "debug", false);
        assert_command_uses_profile(&graph, "moonc check", &["./main/main.mbt"], "debug", false);
    }
}

#[test]
fn build_graph_uses_selected_profile() {
    let fixture = PlanningFixture::new("debug_flag_test").expect("fixture should resolve");

    for (label, args, profile) in [
        (
            "default build",
            ["build", "--dry-run", "--nostd", "--sort-input"].as_slice(),
            "debug",
        ),
        (
            "release build",
            ["build", "--dry-run", "--release", "--nostd", "--sort-input"].as_slice(),
            "release",
        ),
        (
            "debug build",
            ["build", "--dry-run", "--debug", "--nostd", "--sort-input"].as_slice(),
            "debug",
        ),
        (
            "default build with explicit target",
            [
                "build",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--nostd",
                "--sort-input",
            ]
            .as_slice(),
            "debug",
        ),
        (
            "release build with explicit target",
            [
                "build",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--release",
                "--nostd",
                "--sort-input",
            ]
            .as_slice(),
            "release",
        ),
        (
            "debug build with explicit target",
            [
                "build",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--debug",
                "--nostd",
                "--sort-input",
            ]
            .as_slice(),
            "debug",
        ),
    ] {
        let (cli, cmd) = parse_build_command(args);
        let graph = fixture
            .plan_build_with_cli(&cli, &cmd)
            .unwrap_or_else(|err| panic!("{label} should plan: {err:#}"));

        assert_command_uses_profile(
            &graph,
            "moonc build-package",
            &["./lib/hello.mbt"],
            profile,
            profile == "debug",
        );
        assert_command_uses_profile(
            &graph,
            "moonc link-core",
            &["-main", "hello/main"],
            profile,
            profile == "debug",
        );
    }
}

#[test]
fn run_graph_uses_selected_profile() {
    let fixture = PlanningFixture::new("debug_flag_test").expect("fixture should resolve");

    for (label, args, profile) in [
        (
            "default run",
            ["run", "main", "--dry-run", "--nostd", "--sort-input"].as_slice(),
            "debug",
        ),
        (
            "release run",
            [
                "run",
                "main",
                "--dry-run",
                "--release",
                "--nostd",
                "--sort-input",
            ]
            .as_slice(),
            "release",
        ),
        (
            "debug run",
            [
                "run",
                "main",
                "--dry-run",
                "--debug",
                "--nostd",
                "--sort-input",
            ]
            .as_slice(),
            "debug",
        ),
        (
            "default run with explicit target",
            [
                "run",
                "main",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--nostd",
                "--sort-input",
            ]
            .as_slice(),
            "debug",
        ),
        (
            "release run with explicit target",
            [
                "run",
                "main",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--release",
                "--nostd",
                "--sort-input",
            ]
            .as_slice(),
            "release",
        ),
        (
            "debug run with explicit target",
            [
                "run",
                "main",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--debug",
                "--nostd",
                "--sort-input",
            ]
            .as_slice(),
            "debug",
        ),
    ] {
        let (cli, cmd) = parse_run_command(args);
        let graph = fixture
            .plan_run_with_cli(&cli, &cmd)
            .unwrap_or_else(|err| panic!("{label} should plan: {err:#}"));

        assert_command_uses_profile(
            &graph,
            "moonc build-package",
            &["./lib/hello.mbt"],
            profile,
            profile == "debug",
        );
        assert_command_uses_profile(
            &graph,
            "moonc link-core",
            &["-main", "hello/main"],
            profile,
            profile == "debug",
        );
    }
}
