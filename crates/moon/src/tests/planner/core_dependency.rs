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

use moonbuild_rupes_recta::{ResolvedProject, model::TargetKind};
use moonutil::{target::TargetBackend, user_log::UserLog};

use super::fixture::{GraphAction, PlanningFixture, graph_actions, parse_build_command};

// These core rules are backend-independent. One explicit backend keeps paths
// deterministic; conditional imports and backend layouts have their own tests.

fn assert_implicit_import(resolved: &ResolvedProject, package: &str, alias: &str) {
    let app = resolved
        .pkg_dirs
        .get_package_id_by_name("username/hello/lib")
        .unwrap();
    let core = resolved.pkg_dirs.get_package_id_by_name(package).unwrap();
    let dependencies = &resolved.package_relations[&TargetBackend::WasmGC].dep_graph;

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
            .unwrap_or_else(|| panic!("{kind:?}: missing {package}"));
        assert_eq!(edge.short_alias.as_str(), alias);
        assert!(!edge.import_all);
    }
}

#[test]
fn resolving_an_ordinary_package_adds_builtin_with_its_full_alias() {
    let fixture = PlanningFixture::with_core_dependency("test_release").unwrap();
    let log = UserLog::new(log::LevelFilter::Error);
    let (_, resolved) = fixture
        .select_and_resolve(Some(TargetBackend::WasmGC), &log)
        .unwrap();
    assert_implicit_import(
        &resolved,
        "moonbitlang/core/builtin",
        "moonbitlang/core/builtin",
    );
}

fn build_graph(fixture: &PlanningFixture) -> Vec<GraphAction> {
    let (cli, cmd) =
        parse_build_command(&["build", "--target", "wasm-gc", "--sort-input", "--dry-run"]);
    // Run the real planner and lowering to produce an execution plan.
    graph_actions(&fixture.plan_build_with_cli(&cli, &cmd).unwrap())
}

fn application_compile_actions(graph: &[GraphAction]) -> Vec<&GraphAction> {
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
    let fixture = PlanningFixture::with_core_dependency("test_release").unwrap();
    let graph = build_graph(&fixture);
    // Then both app compile actions declare the installed builtin interface as an input.
    for action in application_compile_actions(&graph) {
        assert!(
            action.inputs.iter().any(|path| path
                == "$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/builtin/builtin.mi"),
            "{}",
            action.command
        );
    }
}

#[test]
fn compile_command_passes_builtin_with_its_full_alias() {
    let fixture = PlanningFixture::with_core_dependency("test_release").unwrap();
    let graph = build_graph(&fixture);
    let interface = "$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/builtin/builtin.mi:";
    for action in application_compile_actions(&graph) {
        let args = shlex::split(&action.command).unwrap();
        let aliases: Vec<_> = args
            .windows(2)
            .filter(|pair| pair[0] == "-i")
            .filter_map(|pair| pair[1].strip_prefix(interface))
            .collect();
        // Then builtin is passed exactly once, under its full alias only.
        assert_eq!(aliases, ["moonbitlang/core/builtin"], "{}", action.command);
    }
}

/// Rule: ordinary packages implicitly consume the installed prelude as `prelude`.
///
/// Scenario: neither the app library nor its main package declares a prelude import.
/// The `test_release` fixture supplies a library with source, inline, whitebox and
/// blackbox test targets, plus a main package that imports the library. Fixed core
/// declarations provide the installed standard library; this is not a core build.
///
/// Each test observes one phase of this scenario independently: package resolution,
/// declared execution inputs, or compiler arguments. These checks do not execute
/// the compiler or prove that a later interface change triggers a rebuild.
mod ordinary_packages_without_a_prelude_import {
    use std::path::Path;

    use moonutil::manifest::read_package_desc_file_in_dir;

    use super::*;

    fn given_packages_without_a_prelude_import() -> PlanningFixture {
        let case = "test_release";
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/test_cases")
            .join(case);
        let log = UserLog::new(log::LevelFilter::Error);
        // Guard the Given before resolution: an explicit import could make all
        // three Then assertions pass without exercising implicit injection.
        for package in ["lib", "main"] {
            let manifest = read_package_desc_file_in_dir(&root.join(package), false, &log).unwrap();
            assert!(
                manifest
                    .imports
                    .iter()
                    .chain(&manifest.wbtest_imports)
                    .chain(&manifest.test_imports)
                    .all(|import| import.get_path() != "moonbitlang/core/prelude"),
                "scenario requires {package} to have no declared prelude import"
            );
        }
        PlanningFixture::with_core_dependency(case).unwrap()
    }

    #[test]
    fn resolution_adds_prelude_with_its_short_alias() {
        let fixture = given_packages_without_a_prelude_import();

        // When package imports are resolved for the selected backend.
        let log = UserLog::new(log::LevelFilter::Error);
        let (_, resolved) = fixture
            .select_and_resolve(Some(TargetBackend::WasmGC), &log)
            .unwrap();

        // Then the library's source, inline, whitebox and blackbox targets all
        // import prelude under its short alias, without a glob import.
        assert_implicit_import(&resolved, "moonbitlang/core/prelude", "prelude");
    }

    #[test]
    fn compilation_declares_the_installed_prelude_interface_as_an_input() {
        let fixture = given_packages_without_a_prelude_import();

        // When the build is lowered to an execution plan.
        let graph = build_graph(&fixture);

        // Then both app compile actions track the installed prelude interface.
        for action in application_compile_actions(&graph) {
            assert!(
                action.inputs.iter().any(|path| path
                    == "$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi"),
                "{}",
                action.command
            );
        }
    }

    #[test]
    fn compiler_receives_the_installed_prelude_once_with_its_short_alias() {
        let fixture = given_packages_without_a_prelude_import();

        // When the build is lowered to compiler commands.
        let graph = build_graph(&fixture);

        // Then each app compile command passes that interface exactly once as `prelude`.
        let interface = "$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:";
        for action in application_compile_actions(&graph) {
            let args = shlex::split(&action.command).unwrap();
            let aliases: Vec<_> = args
                .windows(2)
                .filter(|pair| pair[0] == "-i")
                .filter_map(|pair| pair[1].strip_prefix(interface))
                .collect();
            assert_eq!(aliases, ["prelude"], "{}", action.command);
        }
    }
}

#[test]
fn link_action_reads_prebuilt_core_artifacts() {
    let fixture = PlanningFixture::with_core_dependency("test_release").unwrap();
    let graph = build_graph(&fixture);
    let links: Vec<_> = graph
        .iter()
        .filter(|action| action.command.starts_with("moonc link-core "))
        .collect();
    assert_eq!(links.len(), 1, "only the app main package should link");
    let action = links[0];
    let bundle = "$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/";
    let mut core_inputs: Vec<_> = action
        .inputs
        .iter()
        .filter_map(|path| path.strip_prefix(bundle))
        .collect();
    core_inputs.sort_unstable();
    assert_eq!(
        core_inputs,
        ["builtin/builtin.core", "prelude/prelude.core",]
    );
}

#[test]
fn link_command_passes_prebuilt_core_artifacts() {
    let fixture = PlanningFixture::with_core_dependency("test_release").unwrap();
    let graph = build_graph(&fixture);
    let links: Vec<_> = graph
        .iter()
        .filter(|action| action.command.starts_with("moonc link-core "))
        .collect();
    assert_eq!(links.len(), 1, "only the app main package should link");
    let bundle = "$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/";
    // Then the linker reads builtin/prelude from the installed bundle,
    // rather than expecting the consumer to compile core locally.
    let args = shlex::split(&links[0].command).unwrap();
    let mut core_args: Vec<_> = args
        .iter()
        .filter_map(|arg| arg.strip_prefix(bundle))
        .collect();
    core_args.sort_unstable();
    assert_eq!(core_args, ["builtin/builtin.core", "prelude/prelude.core"]);
}

#[test]
fn installed_core_does_not_get_local_compile_actions() {
    let fixture = PlanningFixture::with_core_dependency("test_release").unwrap();
    let graph = build_graph(&fixture);
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
