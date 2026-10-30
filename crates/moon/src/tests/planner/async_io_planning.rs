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

use super::fixture::{PlanningFixture, parse_check_command, parse_test_command};

fn package_compiler_commands(graph: &str, subcommand: &str, source_dir: &str) -> Vec<Vec<String>> {
    graph
        .lines()
        .filter_map(|line| {
            let line: serde_json::Value = serde_json::from_str(line).ok()?;
            let command = line["command"].as_str()?;
            let tokens = shlex::split(command)?;
            (tokens.get(1).map(String::as_str) == Some(subcommand)
                && tokens.iter().any(|token| token.starts_with(source_dir)))
            .then_some(tokens)
        })
        .collect()
}

fn assert_async_io_flag(commands: &[Vec<String>], expected: bool) {
    assert!(!commands.is_empty(), "expected compiler commands");
    for command in commands {
        assert_eq!(
            command.iter().any(|arg| arg == "-use-async-io"),
            expected,
            "unexpected async I/O flag in {command:?}"
        );
    }
}

#[test]
fn check_propagates_package_async_io_to_test_targets() {
    let fixture = PlanningFixture::new("use_async_io").expect("fixture should resolve");
    let (cli, cmd) = parse_check_command(&[
        "check",
        "--target",
        "wasm-gc",
        "--dry-run",
        "--nostd",
        "--sort-input",
    ]);
    let graph = fixture
        .plan_check_with_cli(&cli, &cmd)
        .expect("check graph should plan");

    let enabled = package_compiler_commands(&graph, "check", "./enabled/");
    assert_async_io_flag(&enabled, true);
    assert!(
        enabled
            .iter()
            .any(|command| command.iter().any(|arg| arg == "-whitebox-test"))
    );
    assert!(
        enabled
            .iter()
            .any(|command| command.iter().any(|arg| arg == "-blackbox-test"))
    );

    let defaulted = package_compiler_commands(&graph, "check", "./defaulted/");
    assert_async_io_flag(&defaulted, false);
}

#[test]
fn build_package_propagates_package_async_io_to_all_test_targets() {
    let fixture = PlanningFixture::new("use_async_io").expect("fixture should resolve");
    let (cli, cmd) = parse_test_command(&[
        "test",
        "--target",
        "wasm-gc",
        "--dry-run",
        "--nostd",
        "--sort-input",
    ]);
    let graph = fixture
        .plan_test_with_cli(&cli, &cmd)
        .expect("test graph should plan");

    let enabled = package_compiler_commands(&graph, "build-package", "./enabled/");
    assert_async_io_flag(&enabled, true);
    assert!(
        enabled
            .iter()
            .any(|command| command.iter().any(|arg| arg == "-whitebox-test"))
    );
    assert!(
        enabled
            .iter()
            .any(|command| command.iter().any(|arg| arg == "-blackbox-test"))
    );
    assert!(enabled.iter().any(|command| {
        command.iter().any(|arg| arg == "-test-mode")
            && !command.iter().any(|arg| arg == "-whitebox-test")
            && !command.iter().any(|arg| arg == "-blackbox-test")
    }));

    let defaulted = package_compiler_commands(&graph, "build-package", "./defaulted/");
    assert_async_io_flag(&defaulted, false);
}
