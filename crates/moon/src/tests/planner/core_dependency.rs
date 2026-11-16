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

use moonbuild_rupes_recta::model::TargetKind;
use moonutil::{target::TargetBackend, user_log::UserLog};

use super::fixture::{PlanningFixture, parse_build_command};

fn assert_implicit_import(package: &str, alias: &str) {
    // Given an ordinary project whose source and test manifests declare no core imports.
    let fixture = PlanningFixture::with_core_dependency("test_release").unwrap();
    let log = UserLog::new(log::LevelFilter::Error);
    for backend in [TargetBackend::WasmGC, TargetBackend::Js] {
        // When package imports are resolved for this backend.
        let (_, resolved) = fixture.select_and_resolve(Some(backend), &log).unwrap();
        let app = resolved
            .pkg_dirs
            .get_package_id_by_name("username/hello/lib")
            .unwrap();
        let core = resolved.pkg_dirs.get_package_id_by_name(package).unwrap();
        let dependencies = &resolved.package_relations[&backend].dep_graph;

        // Then source and every candidate test kind depend on the core source
        // interface with the intended alias. Resolution creates these edges
        // before the planner selects which test artifacts to build.
        for kind in [
            TargetKind::Source,
            TargetKind::InlineTest,
            TargetKind::WhiteboxTest,
            TargetKind::BlackboxTest,
        ] {
            let edge = dependencies
                .edge_weight(
                    app.build_target(kind),
                    core.build_target(TargetKind::Source),
                )
                .unwrap_or_else(|| panic!("{backend:?} {kind:?}: missing {package}"));
            assert_eq!(edge.short_alias.as_str(), alias);
            assert!(!edge.import_all);
        }
    }
}

#[test]
fn resolving_an_ordinary_package_adds_builtin_with_its_full_alias() {
    assert_implicit_import("moonbitlang/core/builtin", "moonbitlang/core/builtin");
}

#[test]
fn resolving_an_ordinary_package_adds_prelude_with_its_short_alias() {
    assert_implicit_import("moonbitlang/core/prelude", "prelude");
}

#[derive(serde::Deserialize)]
struct Action {
    command: String,
    inputs: Vec<String>,
    outputs: Vec<String>,
}

fn build_graph(backend: &str) -> Vec<Action> {
    // Given two ordinary app packages and fixed installed-core declarations.
    let fixture = PlanningFixture::with_core_dependency("test_release").unwrap();
    let (cli, cmd) =
        parse_build_command(&["build", "--target", backend, "--sort-input", "--dry-run"]);
    // When the real planner and lowering produce the executable build graph.
    fixture
        .plan_build_with_cli(&cli, &cmd)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn application_compile_actions(graph: &[Action]) -> Vec<&Action> {
    let actions: Vec<_> = graph
        .iter()
        .filter(|action| action.command.starts_with("moonc build-package "))
        .collect();
    assert_eq!(
        actions.len(),
        2,
        "the app library and main package should compile"
    );
    actions
}

#[test]
fn compile_action_tracks_the_implicit_builtin_interface_input() {
    for backend in ["wasm-gc", "js"] {
        let graph = build_graph(backend);
        // Then changing builtin's installed interface invalidates both app packages.
        for action in application_compile_actions(&graph) {
            assert!(
                action.inputs.contains(&format!(
                    "$MOON_HOME/lib/core/_build/{backend}/release/bundle/builtin/builtin.mi"
                )),
                "{}",
                action.command
            );
        }
    }
}

#[test]
fn compile_action_tracks_the_implicit_prelude_interface_input() {
    for backend in ["wasm-gc", "js"] {
        let graph = build_graph(backend);
        // Then changing prelude's installed interface invalidates both app packages.
        for action in application_compile_actions(&graph) {
            assert!(
                action.inputs.contains(&format!(
                    "$MOON_HOME/lib/core/_build/{backend}/release/bundle/prelude/prelude.mi"
                )),
                "{}",
                action.command
            );
        }
    }
}

#[test]
fn compile_command_passes_builtin_with_its_full_alias() {
    for backend in ["wasm-gc", "js"] {
        let graph = build_graph(backend);
        let interface =
            format!("$MOON_HOME/lib/core/_build/{backend}/release/bundle/builtin/builtin.mi:");
        for action in application_compile_actions(&graph) {
            let args = shlex::split(&action.command).unwrap();
            let aliases: Vec<_> = args
                .windows(2)
                .filter(|pair| pair[0] == "-i")
                .filter_map(|pair| pair[1].strip_prefix(&interface))
                .collect();
            // Then builtin is passed exactly once, under its full alias only.
            assert_eq!(aliases, ["moonbitlang/core/builtin"], "{}", action.command);
        }
    }
}

#[test]
fn compile_command_passes_prelude_with_its_short_alias() {
    for backend in ["wasm-gc", "js"] {
        let graph = build_graph(backend);
        let interface =
            format!("$MOON_HOME/lib/core/_build/{backend}/release/bundle/prelude/prelude.mi:");
        for action in application_compile_actions(&graph) {
            let args = shlex::split(&action.command).unwrap();
            let aliases: Vec<_> = args
                .windows(2)
                .filter(|pair| pair[0] == "-i")
                .filter_map(|pair| pair[1].strip_prefix(&interface))
                .collect();
            // Then prelude is passed exactly once, under its short alias only.
            assert_eq!(aliases, ["prelude"], "{}", action.command);
        }
    }
}

#[test]
fn compile_inputs_track_the_reachable_core_interface_closure_once() {
    for backend in ["wasm-gc", "js"] {
        // Given prelude -> builtin/test -> builtin, plus an unreachable package.
        let graph = build_graph(backend);
        let bundle = format!("$MOON_HOME/lib/core/_build/{backend}/release/bundle/");
        for action in application_compile_actions(&graph) {
            let mut core_inputs: Vec<_> = action
                .inputs
                .iter()
                .filter_map(|path| path.strip_prefix(&bundle))
                .collect();
            core_inputs.sort_unstable();
            // Then transitive test.mi is tracked, shared builtin.mi occurs once,
            // and the unrelated package is excluded, for each app compile action.
            assert_eq!(
                core_inputs,
                ["builtin/builtin.mi", "prelude/prelude.mi", "test/test.mi"],
                "{}",
                action.command
            );

            // Transitive invalidation inputs must not become direct compiler imports.
            let args = shlex::split(&action.command).unwrap();
            assert!(
                !args
                    .windows(2)
                    .any(|pair| pair[0] == "-i" && pair[1].starts_with(&format!("{bundle}test/")))
            );
        }
    }
}

#[test]
fn link_action_tracks_the_reachable_installed_core_artifact_inputs() {
    for backend in ["wasm-gc", "js"] {
        let graph = build_graph(backend);
        let links: Vec<_> = graph
            .iter()
            .filter(|action| action.command.starts_with("moonc link-core "))
            .collect();
        assert_eq!(links.len(), 1, "only the app main package should link");
        let action = links[0];
        let bundle = format!("$MOON_HOME/lib/core/_build/{backend}/release/bundle/");
        let mut core_inputs: Vec<_> = action
            .inputs
            .iter()
            .filter_map(|path| path.strip_prefix(&bundle))
            .collect();
        core_inputs.sort_unstable();
        assert_eq!(
            core_inputs,
            [
                "builtin/builtin.core",
                "prelude/prelude.core",
                "test/test.core"
            ]
        );
    }
}

#[test]
fn link_command_passes_the_reachable_installed_core_artifacts_in_dependency_order() {
    for backend in ["wasm-gc", "js"] {
        let graph = build_graph(backend);
        let links: Vec<_> = graph
            .iter()
            .filter(|action| action.command.starts_with("moonc link-core "))
            .collect();
        assert_eq!(links.len(), 1, "only the app main package should link");
        let bundle = format!("$MOON_HOME/lib/core/_build/{backend}/release/bundle/");
        // Then the linker receives each reachable installed artifact in
        // dependency order; no locally built or unreachable core artifact enters.
        let args = shlex::split(&links[0].command).unwrap();
        let core_args: Vec<_> = args
            .iter()
            .filter_map(|arg| arg.strip_prefix(&bundle))
            .collect();
        assert_eq!(
            core_args,
            [
                "builtin/builtin.core",
                "test/test.core",
                "prelude/prelude.core"
            ]
        );
    }
}

#[test]
fn installed_core_does_not_get_local_compile_actions() {
    for backend in ["wasm-gc", "js"] {
        let graph = build_graph(backend);
        let mut packages: Vec<_> = graph
            .iter()
            .filter(|action| action.command.starts_with("moonc build-package "))
            .map(|action| {
                let args = shlex::split(&action.command).unwrap();
                args.windows(2).find(|pair| pair[0] == "-pkg").unwrap()[1].clone()
            })
            .collect();
        packages.sort_unstable();
        // Then only app packages compile; installed core remains read-only input.
        assert_eq!(packages, ["username/hello/lib", "username/hello/main"]);
        assert!(
            graph
                .iter()
                .flat_map(|action| &action.outputs)
                .all(|path| !path.starts_with("$MOON_HOME/lib/core/"))
        );
    }
}
