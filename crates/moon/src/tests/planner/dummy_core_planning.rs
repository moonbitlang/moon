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

use super::fixture::{
    PlanningFixture, parse_build_command, parse_check_command, parse_test_command,
};

// Phase 3: `dummy_core` is mostly a dry-run matrix over one fixture, so planner
// tests can resolve it once and snapshot the resulting graph directly.

#[test]
fn dummy_core_check_graph_matches_snapshots() {
    let fixture = PlanningFixture::new("dummy_core").expect("fixture should resolve");

    let (cli, cmd) = parse_check_command(&["check", "--dry-run", "--sort-input"]);
    expect_file!["../../../tests/test_cases/dummy_core/check_default.jsonl.snap"].assert_eq(
        &fixture
            .plan_check_with_cli(&cli, &cmd)
            .expect("default check graph should plan"),
    );

    let (cli, cmd) =
        parse_check_command(&["check", "--dry-run", "--target", "wasm", "--sort-input"]);
    expect_file!["../../../tests/test_cases/dummy_core/check_wasm.jsonl.snap"].assert_eq(
        &fixture
            .plan_check_with_cli(&cli, &cmd)
            .expect("wasm check graph should plan"),
    );

    let (cli, cmd) =
        parse_check_command(&["check", "--dry-run", "--target", "wasm-gc", "--sort-input"]);
    expect_file!["../../../tests/test_cases/dummy_core/check_wasm_gc.jsonl.snap"].assert_eq(
        &fixture
            .plan_check_with_cli(&cli, &cmd)
            .expect("wasm-gc check graph should plan"),
    );

    let (cli, cmd) = parse_check_command(&["check", "--dry-run", "--target", "js", "--sort-input"]);
    expect_file!["../../../tests/test_cases/dummy_core/check_js.jsonl.snap"].assert_eq(
        &fixture
            .plan_check_with_cli(&cli, &cmd)
            .expect("js check graph should plan"),
    );
}

#[test]
fn dummy_core_build_graph_matches_snapshots() {
    let fixture = PlanningFixture::new("dummy_core").expect("fixture should resolve");

    let (cli, cmd) = parse_build_command(&["build", "--dry-run", "--sort-input"]);
    expect_file!["../../../tests/test_cases/dummy_core/build_default.jsonl.snap"].assert_eq(
        &fixture
            .plan_build_with_cli(&cli, &cmd)
            .expect("default build graph should plan"),
    );

    let (cli, cmd) =
        parse_build_command(&["build", "--dry-run", "--target", "wasm", "--sort-input"]);
    expect_file!["../../../tests/test_cases/dummy_core/build_wasm.jsonl.snap"].assert_eq(
        &fixture
            .plan_build_with_cli(&cli, &cmd)
            .expect("wasm build graph should plan"),
    );

    let (cli, cmd) =
        parse_build_command(&["build", "--dry-run", "--target", "wasm-gc", "--sort-input"]);
    expect_file!["../../../tests/test_cases/dummy_core/build_wasm_gc.jsonl.snap"].assert_eq(
        &fixture
            .plan_build_with_cli(&cli, &cmd)
            .expect("wasm-gc build graph should plan"),
    );

    let (cli, cmd) = parse_build_command(&["build", "--dry-run", "--target", "js", "--sort-input"]);
    expect_file!["../../../tests/test_cases/dummy_core/build_js.jsonl.snap"].assert_eq(
        &fixture
            .plan_build_with_cli(&cli, &cmd)
            .expect("js build graph should plan"),
    );
}

#[test]
fn dummy_core_test_graph_matches_snapshots() {
    let fixture = PlanningFixture::new("dummy_core").expect("fixture should resolve");

    let (cli, cmd) = parse_test_command(&["test", "--dry-run", "--sort-input"]);
    expect_file!["../../../tests/test_cases/dummy_core/test_default.jsonl.snap"].assert_eq(
        &fixture
            .plan_test_with_cli(&cli, &cmd)
            .expect("default test graph should plan"),
    );

    let (cli, cmd) = parse_test_command(&["test", "--dry-run", "--target", "wasm", "--sort-input"]);
    expect_file!["../../../tests/test_cases/dummy_core/test_wasm.jsonl.snap"].assert_eq(
        &fixture
            .plan_test_with_cli(&cli, &cmd)
            .expect("wasm test graph should plan"),
    );

    let (cli, cmd) =
        parse_test_command(&["test", "--dry-run", "--target", "wasm-gc", "--sort-input"]);
    expect_file!["../../../tests/test_cases/dummy_core/test_wasm_gc.jsonl.snap"].assert_eq(
        &fixture
            .plan_test_with_cli(&cli, &cmd)
            .expect("wasm-gc test graph should plan"),
    );

    let (cli, cmd) = parse_test_command(&["test", "--dry-run", "--target", "js", "--sort-input"]);
    expect_file!["../../../tests/test_cases/dummy_core/test_js.jsonl.snap"].assert_eq(
        &fixture
            .plan_test_with_cli(&cli, &cmd)
            .expect("js test graph should plan"),
    );
}
