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

use std::path::Path;

use moonbuild::prepare::ProjectPreparationConfig;
use moonbuild_rupes_recta::{DiscoveredProject, model::TargetKind};
use moonutil::{
    constants::MOONBITLANG_CORE,
    project::{SourceTargetDirs, WorkspaceEnv},
    resolution::ModuleSourceKind,
    target::TargetBackend,
    user_log::UserLog,
};

use super::fixture::{
    PlanningFixture, parse_build_command, parse_test_command, planned_graph_inputs,
};

fn discover_core(enable_coverage: bool) -> anyhow::Result<DiscoveredProject> {
    let source_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases/core_project");
    let user_log = UserLog::new(log::LevelFilter::Error);
    let dirs = SourceTargetDirs {
        cwd: None,
        target_dir: None,
    }
    .query_from(&source_dir, WorkspaceEnv::Off)?
    .select(&user_log)?
    .package_dirs()?;
    let preparation = ProjectPreparationConfig::new_with_load_defaults(true, enable_coverage);
    moonbuild::prepare::sync_and_discover_project(&preparation, &dirs, &user_log)
}

fn package_source_paths(project: &DiscoveredProject, name: &str, root: &Path) -> Vec<String> {
    let package_id = project
        .pkg_dirs
        .get_package_id_by_name(name)
        .unwrap_or_else(|| panic!("fixture is missing package {name}"));
    let mut paths: Vec<_> = project
        .pkg_dirs
        .get_package(package_id)
        .source_files
        .iter()
        .map(|path| {
            path.strip_prefix(root)
                .expect("discovered source should be under the fixture root")
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    paths.sort();
    paths
}

#[test]
fn core_root_is_not_given_a_second_stdlib_module() -> anyhow::Result<()> {
    // Given moonbitlang/core as the input project, run module sync and discovery.
    let project = discover_core(false)?;

    // Then core remains the only input module and keeps its local source role.
    assert_eq!(project.module_graph.module_count(), 1);
    assert_eq!(project.module_graph.input_module_ids().len(), 1);
    let root_id = project.module_graph.input_module_ids()[0];
    assert_eq!(
        project
            .module_graph
            .module_source(root_id)
            .name()
            .to_string(),
        MOONBITLANG_CORE
    );
    assert!(matches!(
        project.module_graph.module_source(root_id).source(),
        ModuleSourceKind::Local(_)
    ));
    assert!(project.module_dirs.contains_key(root_id));
    assert_eq!(project.local_modules(), &[root_id]);
    Ok(())
}

#[test]
fn coverage_sources_join_builtin_only_when_coverage_is_enabled() -> anyhow::Result<()> {
    // Given the same checked-in core module, discover packages with both coverage modes.
    let without_coverage = discover_core(false)?;
    let with_coverage = discover_core(true)?;
    let root_id = with_coverage.local_modules()[0];
    // Match discovery's canonical paths, including Windows verbatim prefixes.
    let root = std::fs::canonicalize(
        with_coverage
            .module_dirs
            .get(root_id)
            .expect("root module should have a synchronized directory"),
    )?;

    // Then coverage source stays in its own package unless the root-core rule merges it.
    assert_eq!(
        package_source_paths(&without_coverage, "moonbitlang/core/builtin", &root),
        vec!["builtin/builtin.mbt".to_owned()]
    );
    assert_eq!(
        package_source_paths(&with_coverage, "moonbitlang/core/builtin", &root),
        vec![
            "builtin/builtin.mbt".to_owned(),
            "coverage/coverage.mbt".to_owned()
        ]
    );
    assert_eq!(
        package_source_paths(&with_coverage, "moonbitlang/core/coverage", &root),
        vec!["coverage/coverage.mbt".to_owned()]
    );

    // Builtin and coverage need no explicit imports to discover or merge sources.
    for project in [&without_coverage, &with_coverage] {
        for name in ["moonbitlang/core/builtin", "moonbitlang/core/coverage"] {
            let package_id = project.pkg_dirs.get_package_id_by_name(name).unwrap();
            assert!(
                project
                    .pkg_dirs
                    .get_package(package_id)
                    .raw
                    .imports
                    .is_empty()
            );
        }
    }

    Ok(())
}

#[test]
fn coverage_edges_exclude_builtin_and_coverage() -> anyhow::Result<()> {
    // Given a core project with coverage enabled, resolve package relationships.
    let discovered = discover_core(true)?;
    let builtin = discovered
        .pkg_dirs
        .get_package_id_by_name("moonbitlang/core/builtin")
        .unwrap();
    let coverage = discovered
        .pkg_dirs
        .get_package_id_by_name("moonbitlang/core/coverage")
        .unwrap();
    let probe = discovered
        .pkg_dirs
        .get_package_id_by_name("moonbitlang/core/probe")
        .unwrap();
    let user_log = UserLog::new(log::LevelFilter::Error);
    let resolved = discovered.resolve_packages(&[TargetBackend::WasmGC], &user_log)?;
    let relations = &resolved.package_relations[&TargetBackend::WasmGC];

    // Then regular core packages use coverage, while builtin and coverage do not self-import it.
    assert!(relations.dep_graph.contains_edge(
        probe.build_target(TargetKind::Source),
        coverage.build_target(TargetKind::Source),
    ));
    for package in [builtin, coverage] {
        assert!(!relations.dep_graph.contains_edge(
            package.build_target(TargetKind::Source),
            coverage.build_target(TargetKind::Source),
        ));
    }
    Ok(())
}

#[test]
fn core_packages_import_prelude_only_for_blackbox_tests() -> anyhow::Result<()> {
    // Given core itself, including a probe package with an actual blackbox test.
    let discovered = discover_core(false)?;
    let prelude = discovered
        .pkg_dirs
        .get_package_id_by_name("moonbitlang/core/prelude")
        .unwrap();
    let user_log = UserLog::new(log::LevelFilter::Error);
    // When package imports are resolved.
    let resolved = discovered.resolve_packages(&[TargetBackend::WasmGC], &user_log)?;
    let relations = &resolved.package_relations[&TargetBackend::WasmGC];

    // Then both builtin and an ordinary core package get prelude only for blackbox tests.
    for name in ["moonbitlang/core/builtin", "moonbitlang/core/probe"] {
        let package = resolved.pkg_dirs.get_package_id_by_name(name).unwrap();
        let manifest = &resolved.pkg_dirs.get_package(package).raw;
        assert!(manifest.imports.is_empty());
        assert_eq!(manifest.test_imports.len(), 1);
        assert_eq!(
            relations
                .dep_graph
                .edge_weight(
                    package.build_target(TargetKind::BlackboxTest),
                    prelude.build_target(TargetKind::Source),
                )
                .map(|edge| edge.short_alias.as_str()),
            Some("prelude"),
            "{name}"
        );
        for kind in [
            TargetKind::Source,
            TargetKind::InlineTest,
            TargetKind::WhiteboxTest,
        ] {
            assert!(
                !relations.dep_graph.contains_edge(
                    package.build_target(kind),
                    prelude.build_target(TargetKind::Source),
                ),
                "{name}: {kind:?}"
            );
        }
        assert!(!relations.dep_graph.contains_edge(
            package.build_target(TargetKind::Source),
            package.build_target(TargetKind::Source),
        ));
    }
    Ok(())
}

#[test]
fn core_project_produces_local_core_artifacts() -> anyhow::Result<()> {
    // Given core itself, with local builtin and prelude packages.
    let fixture = PlanningFixture::new("core_project")?;
    let (cli, cmd) = parse_build_command(&[
        "build",
        "--target",
        "wasm-gc",
        "--debug",
        "--sort-input",
        "--dry-run",
    ]);
    // When a build is planned, core artifacts need local producers.
    let graph = fixture.plan_build_with_cli(&cli, &cmd)?;
    let outputs: Vec<String> = graph
        .lines()
        .flat_map(|line| {
            let action: serde_json::Value = serde_json::from_str(line).unwrap();
            action["outputs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|path| path.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        })
        .collect();

    // Then builtin/prelude interfaces and IR are outputs, without installed core inputs.
    for package in ["builtin", "prelude"] {
        for extension in ["mi", "core"] {
            assert!(
                outputs.contains(&format!(
                    "./_build/wasm-gc/debug/build/{package}/{package}.{extension}"
                )),
                "{outputs:?}"
            );
        }
    }
    assert!(
        planned_graph_inputs(&graph)
            .iter()
            .all(|path| !path.starts_with("$MOON_HOME/lib/core/"))
    );
    Ok(())
}

#[test]
fn core_source_commands_do_not_receive_implicit_core_imports() -> anyhow::Result<()> {
    // Given core packages with no explicit imports, unlike an ordinary consumer.
    let fixture = PlanningFixture::new("core_project")?;
    let (cli, cmd) =
        parse_build_command(&["build", "--target", "wasm-gc", "--sort-input", "--dry-run"]);
    // When the planner lowers their source compilation commands.
    let graph = fixture.plan_build_with_cli(&cli, &cmd)?;
    let commands: Vec<String> = graph
        .lines()
        .filter_map(|line| {
            let action: serde_json::Value = serde_json::from_str(line).unwrap();
            let command = action["command"].as_str().unwrap();
            command
                .starts_with("moonc build-package ")
                .then(|| command.to_owned())
        })
        .collect();

    // Then all four local packages compile without implicit builtin/prelude -i flags.
    assert_eq!(commands.len(), 4);
    for command in commands {
        assert!(
            !shlex::split(&command)
                .unwrap()
                .iter()
                .any(|arg| arg == "-i"),
            "{command}"
        );
    }
    Ok(())
}

#[test]
fn core_blackbox_compile_command_imports_local_prelude() -> anyhow::Result<()> {
    // Given a core package with a blackbox test and no declared imports.
    let fixture = PlanningFixture::new("core_project")?;
    let (cli, cmd) = parse_test_command(&[
        "test",
        "--target",
        "wasm-gc",
        "--debug",
        "--sort-input",
        "--dry-run",
    ]);
    // When its test graph is lowered, the special prelude import must reach moonc.
    let graph = fixture.plan_test_with_cli(&cli, &cmd)?;
    let command = graph
        .lines()
        .find_map(|line| {
            let action: serde_json::Value = serde_json::from_str(line).unwrap();
            let command = action["command"].as_str().unwrap();
            command
                .contains("-pkg moonbitlang/core/probe_blackbox_test ")
                .then(|| command.to_owned())
        })
        .expect("the probe blackbox test must have a compile command");
    let args = shlex::split(&command).unwrap();
    let imports: Vec<_> = args
        .windows(2)
        .filter(|pair| pair[0] == "-i")
        .map(|pair| pair[1].as_str())
        .collect();
    // Then both interfaces are local: the package under test and core's prelude.
    assert_eq!(
        imports,
        [
            "./_build/wasm-gc/debug/test/prelude/prelude.mi:prelude",
            "./_build/wasm-gc/debug/test/probe/probe.mi:probe",
        ]
    );
    Ok(())
}
