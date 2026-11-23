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

use super::fixture::{
    PlanningFixture, graph_actions, parse_bench_command, parse_build_command, parse_check_command,
    parse_run_command, parse_test_command,
};

// Phase 3: these tests start from an already chosen command configuration and
// assert how the planner lowers that intent into the dry-run graph.

#[test]
fn profile_controls_sources_flags_and_output_paths_together() {
    // The same JS package has debug-only and release-only sources. Keeping
    // symbols in release must not switch its source selection back to debug.
    let fixture = PlanningFixture::with_core_dependency("cond_comp.in").unwrap();
    for (options, profile, debug_sources, symbols) in [
        (vec!["--debug"], "debug", true, true),
        (vec!["--release"], "release", false, false),
        (vec!["--release", "--no-strip"], "release", false, true),
    ] {
        let args = [vec!["build", "--target", "js"], options].concat();
        let (cli, cmd) = parse_build_command(&args);
        let graph = fixture.plan_build_with_cli(&cli, &cmd).unwrap();
        let actions = graph_actions(&graph);
        let library = actions
            .iter()
            .find(|action| {
                action
                    .outputs
                    .iter()
                    .any(|path| path.ends_with("/lib/lib.core"))
            })
            .expect("the requested build compiles the library");

        for (file, selected) in [
            ("only_debug.mbt", debug_sources),
            ("wasm_release_or_js_debug.mbt", debug_sources),
            ("js_and_release.mbt", !debug_sources),
        ] {
            assert_eq!(
                library
                    .inputs
                    .iter()
                    .any(|path| path.ends_with(&format!("/{file}"))),
                selected,
                "{file} in {profile}"
            );
        }
        let command = shlex::split(&library.command).unwrap();
        assert_eq!(command.iter().any(|arg| arg == "-O0"), debug_sources);
        assert_eq!(command.iter().any(|arg| arg == "-g"), symbols);
        assert!(
            library
                .outputs
                .iter()
                .any(|path| { path.ends_with(&format!("/js/{profile}/build/lib/lib.core")) })
        );
    }
}

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
    let (cli, cmd) =
        parse_bench_command(&["bench", "--target", "wasm-gc", "--sort-input", "--dry-run"]);

    let default_graph = fixture
        .plan_bench_with_cli(&cli, &cmd)
        .expect("default bench graph should plan");
    assert!(default_graph.contains("moonc"));
    assert!(!default_graph.contains("-O0"));

    let (release_cli, release_cmd) = parse_bench_command(&[
        "bench",
        "--target",
        "wasm-gc",
        "--release",
        "--sort-input",
        "--dry-run",
    ]);
    let release_graph = fixture
        .plan_bench_with_cli(&release_cli, &release_cmd)
        .expect("release bench graph should plan");
    assert!(release_graph.contains("moonc"));
    assert!(!release_graph.contains("-O0"));

    let (debug_cli, debug_cmd) = parse_bench_command(&[
        "bench",
        "--target",
        "wasm-gc",
        "--debug",
        "--sort-input",
        "--dry-run",
    ]);
    let debug_graph = fixture
        .plan_bench_with_cli(&debug_cli, &debug_cmd)
        .expect("debug bench graph should plan");
    assert!(debug_graph.contains("moonc"));
    assert!(debug_graph.contains("-O0"));
}

fn assert_test_profile(graph: &str, profile: &str) {
    let actions = graph_actions(graph);
    let root = format!("./_build/wasm-gc/{profile}/test/");

    // Both profiles must retain the same source, inline, whitebox and blackbox
    // targets. Compare project artifacts, not the installed core's package graph.
    let mut artifacts: Vec<_> = actions
        .iter()
        .flat_map(|action| &action.outputs)
        .filter(|path| path.ends_with(".core") || path.ends_with(".wasm"))
        .map(|path| {
            path.strip_prefix(&root)
                .unwrap_or_else(|| panic!("wrong profile: {path}"))
        })
        .collect();
    artifacts.sort_unstable();
    assert_eq!(
        artifacts,
        [
            "lib/lib.blackbox_test.core",
            "lib/lib.blackbox_test.wasm",
            "lib/lib.core",
            "lib/lib.internal_test.core",
            "lib/lib.internal_test.wasm",
            "lib/lib.whitebox_test.core",
            "lib/lib.whitebox_test.wasm",
            "main/main.blackbox_test.core",
            "main/main.blackbox_test.wasm",
            "main/main.core",
            "main/main.internal_test.core",
            "main/main.internal_test.wasm",
        ]
    );

    let mut drivers = Vec::new();
    for action in &actions {
        let args = shlex::split(&action.command).unwrap();
        // Generated drivers, metadata, interface inputs and command paths must
        // agree on the profile too; checking only the executable path is insufficient.
        for path in action
            .inputs
            .iter()
            .chain(&action.outputs)
            .chain(&args)
            .filter(|path| path.starts_with("./_build/"))
        {
            assert!(path.starts_with(&root), "wrong profile: {path}");
        }
        match args[1].as_str() {
            "generate-test-driver" => {
                let package = args
                    .windows(2)
                    .find(|pair| pair[0] == "--pkg-name")
                    .unwrap();
                let kind = args
                    .windows(2)
                    .find(|pair| pair[0] == "--driver-kind")
                    .unwrap();
                drivers.push((package[1].clone(), kind[1].clone()));
            }
            "build-package" | "link-core" => {
                // Both profiles retain diagnostic information; release enables
                // optimization as well as changing the output directory.
                for flag in ["-g", "-O0", "-source-map"] {
                    assert_eq!(
                        args.iter().any(|arg| arg == flag),
                        flag != "-O0" || profile == "debug",
                        "{}: {flag}",
                        action.command
                    );
                }
                if args[1] == "link-core" {
                    assert!(args.iter().any(|arg| arg == "-test-mode"));
                    assert_eq!(
                        args.iter().any(|arg| arg == "-wasm-module-name"),
                        profile == "release"
                    );
                }
            }
            other => panic!("unexpected test planning action: {other}"),
        }
    }
    drivers.sort();
    assert_eq!(
        drivers,
        [
            ("username/hello/lib", "blackbox"),
            ("username/hello/lib", "internal"),
            ("username/hello/lib", "whitebox"),
            ("username/hello/main", "blackbox"),
            ("username/hello/main", "internal"),
        ]
        .map(|(package, kind)| (package.to_owned(), kind.to_owned()))
    );
}

#[test]
fn test_defaults_to_debug_codegen_and_artifacts() {
    // Given an ordinary project with inline, whitebox and blackbox test targets.
    let fixture = PlanningFixture::with_core_dependency("test_release").unwrap();
    // When no profile flag is passed to test.
    let (cli, cmd) =
        parse_test_command(&["test", "--target", "wasm-gc", "--sort-input", "--dry-run"]);
    let graph = fixture.plan_test_with_cli(&cli, &cmd).unwrap();
    // Then every target uses the debug profile, including generated drivers.
    assert_test_profile(&graph, "debug");
}

#[test]
fn release_test_keeps_test_targets_with_release_codegen_and_artifacts() {
    // Given the same ordinary project, changing the profile must not drop tests.
    let fixture = PlanningFixture::with_core_dependency("test_release").unwrap();
    // When test is planned with --release.
    let (cli, cmd) = parse_test_command(&[
        "test",
        "--target",
        "wasm-gc",
        "--release",
        "--sort-input",
        "--dry-run",
    ]);
    let graph = fixture.plan_test_with_cli(&cli, &cmd).unwrap();
    // Then the same test targets use release artifacts and code generation.
    assert_test_profile(&graph, "release");
}

#[test]
fn check_graph_uses_debug_profile_without_codegen_flags() {
    let fixture = PlanningFixture::new("debug_flag_test").expect("fixture should resolve");

    for (label, args) in [
        (
            "default check",
            ["check", "--target", "wasm-gc", "--dry-run", "--sort-input"].as_slice(),
        ),
        (
            "explicit debug check",
            [
                "check",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--debug",
                "--sort-input",
            ]
            .as_slice(),
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
            ["build", "--target", "wasm-gc", "--dry-run", "--sort-input"].as_slice(),
            "debug",
        ),
        (
            "release build",
            [
                "build",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--release",
                "--sort-input",
            ]
            .as_slice(),
            "release",
        ),
        (
            "debug build",
            [
                "build",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--debug",
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
            [
                "run",
                "main",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--sort-input",
            ]
            .as_slice(),
            "debug",
        ),
        (
            "release run",
            [
                "run",
                "main",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--release",
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
                "--target",
                "wasm-gc",
                "--dry-run",
                "--debug",
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

#[test]
fn native_run_cli_flags_match_requested_debug_info() {
    for (case, package, target_args) in [
        ("debug_flag_test", "main", ["--target", "native"].as_slice()),
        (
            "workspace_conflicting_run_preferred_targets.in",
            "native_preferred/src/main",
            [].as_slice(),
        ),
    ] {
        let fixture = PlanningFixture::new(case).expect("fixture should resolve");
        for (label, extra_args, expect_debug_info) in [
            ("default", [].as_slice(), None),
            ("debug", ["--debug"].as_slice(), Some(true)),
            ("no-strip", ["--no-strip"].as_slice(), Some(true)),
            ("strip", ["--debug", "--strip"].as_slice(), Some(false)),
        ] {
            let mut args = vec!["run", package, "--dry-run", "--sort-input"];
            args.extend_from_slice(target_args);
            args.extend_from_slice(extra_args);
            let (cli, cmd) = parse_run_command(&args);
            let graph = fixture
                .plan_run_with_cli(&cli, &cmd)
                .unwrap_or_else(|err| panic!("{case}: {label} native run should plan: {err:#}"));

            // CLI tests cover explicit and preferred backend selection. The
            // RR compile tests inject candidates to guarantee each payload form.
            let link = command_tokens(&graph, "moonc link-core", &["-main"]);
            let generates_c = link
                .windows(2)
                .any(|pair| pair[0] == "-o" && pair[1].ends_with(".c"));

            for (command, filter) in [
                ("moonc build-package", ["main.mbt"].as_slice()),
                ("moonc link-core", ["-main"].as_slice()),
            ] {
                let tokens = command_tokens(&graph, command, filter);
                assert!(
                    tokens.iter().any(|token| token.contains("/native/debug/")),
                    "{tokens:?}"
                );
                assert!(tokens.iter().any(|token| token == "-O0"), "{tokens:?}");
                assert_eq!(
                    tokens.iter().any(|token| token == "-stacktrace"),
                    label == "default" && !generates_c,
                    "{case}: {label}: {tokens:?}"
                );
                assert_eq!(
                    tokens.iter().any(|token| token == "-g"),
                    expect_debug_info.unwrap_or(generates_c),
                    "{case}: {label}: {tokens:?}"
                );
            }
        }
    }
}

#[test]
fn stripping_keeps_runtime_backtraces_only_for_debug_profile_runs() {
    let fixture = PlanningFixture::new("debug_flag_test").expect("fixture should resolve");
    for backend in ["native", "llvm"] {
        for (command, extra_args, expect_backtrace) in [
            ("run", [].as_slice(), !cfg!(windows)),
            ("run", ["--release"].as_slice(), false),
            ("build", [].as_slice(), false),
            ("test", [].as_slice(), false),
            ("bench", [].as_slice(), false),
        ] {
            let mut args = vec![command, "--target", backend, "--strip", "--dry-run"];
            args.extend_from_slice(extra_args);
            let graph = match command {
                "run" => {
                    args.push("main");
                    let (cli, cmd) = parse_run_command(&args);
                    fixture.plan_run_with_cli(&cli, &cmd)
                }
                "build" => {
                    let (cli, cmd) = parse_build_command(&args);
                    fixture.plan_build_with_cli(&cli, &cmd)
                }
                "test" => {
                    let (cli, cmd) = parse_test_command(&args);
                    fixture.plan_test_with_cli(&cli, &cmd)
                }
                "bench" => {
                    let (cli, cmd) = parse_bench_command(&args);
                    fixture.plan_bench_with_cli(&cli, &cmd)
                }
                _ => unreachable!(),
            }
            .unwrap_or_else(|err| panic!("{args:?} should plan: {err:#}"));
            let runtime = command_tokens(&graph, "runtime.c", &[]);
            assert_eq!(
                runtime
                    .iter()
                    .any(|arg| arg == "-DMOONBIT_ALLOW_STACKTRACE"),
                expect_backtrace,
                "{args:?}: runtime backtrace support"
            );
            let link = command_tokens(&graph, "moonc link-core", &[]);
            assert!(!link.iter().any(|arg| arg == "-g"), "{args:?}: {link:?}");
        }
    }
}
