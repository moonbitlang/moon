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

#![allow(clippy::disallowed_methods)] // These tests exercise manifest discovery.

use moonbuild_rupes_recta::{
    CompileConfig, ProjectDeclarations, ResolveOutput,
    build_lower::WarningCondition,
    build_plan::{ArtifactKey, InputDirective},
    compile::compile,
    discover::discover_packages,
    metadata::{CheckCommandMap, gen_metadata_json},
    model::{BackendConfig, BuildTarget, DebugInfoRequest, TargetKind},
    resolve::ResolveError,
    target_layout::{ArtifactPathResolver, TargetLayout, TargetLayoutMode},
};
use moonutil::{
    build_options::RunMode,
    cond_expr::OptLevel,
    manifest::read_module_desc_file_in_dir,
    resolution::{DirSyncResult, ModuleSource, ResolvedEnv},
    target::TargetBackend::{self, Js, Native},
    user_log::UserLog,
};

fn resolve(packages: &[(&str, &str)]) -> (tempfile::TempDir, ResolveOutput) {
    let (dir, declarations) = discover(packages);
    let resolved = declarations
        .resolve(
            TargetBackend::all(),
            false,
            &UserLog::new(log::LevelFilter::Error),
        )
        .unwrap();
    (dir, resolved)
}

fn discover(packages: &[(&str, &str)]) -> (tempfile::TempDir, ProjectDeclarations) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("moon.mod"),
        "name = \"example/conditional\"\n",
    )
    .unwrap();
    for (path, manifest) in packages {
        let root = dir.path().join(path);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("moon.pkg"), manifest).unwrap();
    }
    let module = read_module_desc_file_in_dir(dir.path()).unwrap();
    let source = ModuleSource::from_local_module(&module, dir.path()).unwrap();
    let (module_rel, mid) = ResolvedEnv::only_one_module(source, module);
    let mut module_dirs = DirSyncResult::new();
    module_dirs.insert(mid, dir.path().to_owned());
    let log = UserLog::new(log::LevelFilter::Error);
    let pkg_dirs = discover_packages(&module_rel, &module_dirs, &log).unwrap();
    (
        dir,
        ProjectDeclarations {
            module_rel,
            module_dirs,
            pkg_dirs,
        },
    )
}

#[test]
fn resolved_project_already_contains_each_backends_graph() {
    let (_dir, resolved) = resolve(&[
        (
            "app",
            r#"#cfg(target = "native") import { "example/conditional/native" }"#,
        ),
        ("native", ""),
    ]);
    let app = target(&resolved, "app", TargetKind::Source);
    let native = target(&resolved, "native", TargetKind::Source);
    for &backend in TargetBackend::all() {
        assert_eq!(
            resolved.pkg_rel[&backend]
                .dep_graph
                .contains_edge(app, native),
            backend == Native,
        );
    }
}

#[test]
fn conditional_imports_reach_compiler_commands_and_metadata() {
    let (dir, resolved) = resolve(&[
        (
            "app",
            r#"
supported_targets = "all"
#cfg(target = "native")
import { "example/conditional/native" @platform }
#cfg(not(target = "native"))
import { "example/conditional/portable" @platform }
#cfg(target = "native")
import { "example/conditional/helper" } for "wbtest"
#cfg(target = "native")
import { "example/conditional/helper" } for "test"
"#,
        ),
        ("native", r#"supported_targets = "native""#),
        ("portable", r#"supported_targets = "all-native""#),
        ("helper", r#"supported_targets = "native""#),
    ]);
    let log = UserLog::new(log::LevelFilter::Error);
    let app = target(&resolved, "app", TargetKind::Source).package;
    for (backend, implementation) in [
        (
            BackendConfig::Native {
                direct_object_candidate: None,
                allocator: moonutil::compiler_flags::NativeAllocator::System,
                os: std::env::consts::OS.parse().unwrap(),
                compiler_paths: moonutil::compiler_flags::CompilerPaths::from_moon_dirs(),
            },
            "native",
        ),
        (BackendConfig::Js, "portable"),
    ] {
        let target_dir = dir.path().join("_build");
        let config = CompileConfig {
            artifact_paths: ArtifactPathResolver::new(
                TargetLayout::new(
                    target_dir.clone(),
                    TargetLayoutMode::Mono {
                        main_module: resolved
                            .module_rel
                            .module_source(resolved.local_modules()[0])
                            .clone(),
                    },
                    OptLevel::Debug,
                    RunMode::Check,
                ),
                None,
            ),
            target_dir,
            backend,
            opt_level: OptLevel::Debug,
            action: RunMode::Check,
            debug_info: DebugInfoRequest::default(),
            stdlib_path: None,
            debug_export_build_plan: false,
            enable_coverage: false,
            moonc_output_json: false,
            docs_serve: false,
            warning_condition: WarningCondition::Default,
            warn_list: None,
            info_no_alias: false,
        };
        let backend = config.backend.target_backend();
        let expected_mi = format!(
            "{}:platform",
            config
                .artifact_paths
                .mi_of_build_target(
                    &resolved.pkg_dirs,
                    &target(&resolved, implementation, TargetKind::Source),
                    backend,
                )
                .display()
        );
        for kind in [
            TargetKind::Source,
            TargetKind::WhiteboxTest,
            TargetKind::BlackboxTest,
        ] {
            let output = compile(
                &config,
                dir.path(),
                &resolved,
                &[ArtifactKey::CheckMi {
                    package: app,
                    target_kind: kind,
                }],
                &InputDirective::default(),
                None,
                &log,
            )
            .unwrap();
            let commands = output
                .execution_plan
                .action_ids()
                .map(|id| output.execution_plan.action(id).command().args())
                .collect::<Vec<_>>();
            let package_name = if kind == TargetKind::BlackboxTest {
                "example/conditional/app_blackbox_test"
            } else {
                "example/conditional/app"
            };
            let command = commands
                .iter()
                .find(|args| args.windows(2).any(|pair| pair == ["-pkg", package_name]))
                .unwrap();
            let imports = command
                .windows(2)
                .filter(|pair| pair[0] == "-i")
                .map(|pair| pair[1].as_str())
                .collect::<Vec<_>>();
            assert!(imports.contains(&expected_mi.as_str()), "{command:?}");
            assert_eq!(
                imports
                    .iter()
                    .filter(|arg| arg.ends_with(":platform"))
                    .count(),
                1
            );
            assert_eq!(
                imports.iter().any(|arg| arg.ends_with(":helper")),
                backend == Native && kind != TargetKind::Source
            );
        }
        let metadata = gen_metadata_json(
            &resolved,
            dir.path(),
            &config.artifact_paths,
            OptLevel::Debug,
            backend,
            &CheckCommandMap::new(),
        );
        let app_metadata = metadata
            .packages
            .iter()
            .find(|pkg| pkg.rel == "app")
            .unwrap();
        assert_eq!(app_metadata.deps.len(), 1);
        assert_eq!(app_metadata.deps[0].alias, "platform");
        assert_eq!(
            app_metadata.deps[0].path,
            format!("example/conditional/{implementation}")
        );
        assert_eq!(
            app_metadata.wbtest_deps.len(),
            usize::from(backend == Native)
        );
        assert_eq!(app_metadata.test_deps.len(), usize::from(backend == Native));
    }
}

fn target(resolved: &ResolveOutput, name: &str, kind: TargetKind) -> BuildTarget {
    resolved
        .pkg_dirs
        .get_package_id_by_name(&format!("example/conditional/{name}"))
        .unwrap()
        .build_target(kind)
}

fn imports(
    resolved: &ResolveOutput,
    name: &str,
    kind: TargetKind,
    backend: TargetBackend,
) -> Vec<(String, String)> {
    let mut imports = resolved.pkg_rel[&backend]
        .dep_graph
        .edges(target(resolved, name, kind))
        .map(|(_, to, edge)| {
            (
                resolved.pkg_dirs.fqn(to.package).package().to_string(),
                edge.short_alias.to_string(),
            )
        })
        .collect::<Vec<_>>();
    imports.sort();
    imports
}

#[test]
fn conditional_imports_select_aliases_and_propagate_backend_support() {
    let (_dir, resolved) = resolve(&[
        (
            "app",
            r#"
import { "example/conditional/common" }
#cfg(target = "native")
import { "example/conditional/native" @platform }
#cfg(target = "js")
import { "example/conditional/js" @platform }
#cfg(target = "native")
import { "example/conditional/common" @native_common }
"#,
        ),
        ("consumer", r#"import { "example/conditional/app" }"#),
        ("common", ""),
        ("native", r#"supported_targets = "native""#),
        ("js", r#"supported_targets = "js""#),
    ]);
    assert_eq!(
        imports(&resolved, "app", TargetKind::Source, Native),
        [
            ("common".into(), "native_common".into()),
            ("native".into(), "platform".into()),
        ]
    );
    assert_eq!(
        imports(&resolved, "app", TargetKind::Source, Js),
        [
            ("common".into(), "common".into()),
            ("js".into(), "platform".into()),
        ]
    );
    for &backend in TargetBackend::all() {
        let supported = &resolved.pkg_rel[&backend].realizable_supported_targets
            [&target(&resolved, "consumer", TargetKind::Source)];
        assert!(supported.contains(&backend));
    }
}

#[test]
fn conditional_test_imports_preserve_inheritance_and_isolated_targets() {
    let (_dir, mut resolved) = resolve(&[
        (
            "app",
            r#"
#cfg(target = "native")
import { "example/conditional/native" @platform }
#cfg(target = "native")
import { "example/conditional/native" @whitebox } for "wbtest"
#cfg(target = "js")
import { "example/conditional/js" @blackbox } for "test"
"#,
        ),
        ("native", r#"supported_targets = "native""#),
        ("js", r#"supported_targets = "js""#),
    ]);
    assert_eq!(
        imports(&resolved, "app", TargetKind::InlineTest, Native),
        [("native".into(), "platform".into())]
    );
    assert_eq!(
        imports(&resolved, "app", TargetKind::WhiteboxTest, Native),
        [("native".into(), "whitebox".into())]
    );
    assert_eq!(
        imports(&resolved, "app", TargetKind::BlackboxTest, Js),
        [
            ("app".into(), "app".into()),
            ("js".into(), "blackbox".into())
        ]
    );
    assert!(imports(&resolved, "app", TargetKind::WhiteboxTest, Js).is_empty());
    let whitebox = target(&resolved, "app", TargetKind::WhiteboxTest);
    let package = resolved
        .declarations
        .pkg_dirs
        .get_package_mut(whitebox.package);
    let source = package.root_path.join("app_wbtest.mbt");
    std::fs::write(&source, "").unwrap();
    package.source_files.push(source);

    assert!(
        resolved.pkg_rel[&Js]
            .dep_graph
            .edges(whitebox)
            .next()
            .is_none()
    );
    let mut artifacts = Vec::new();
    moonbuild_rupes_recta::intent::UserIntent::Check(whitebox.package).append_artifacts(
        &resolved,
        &mut artifacts,
        &UserLog::new(log::LevelFilter::Error),
        &InputDirective::default(),
        Js,
    );
    assert!(artifacts.contains(&ArtifactKey::CheckMi {
        package: whitebox.package,
        target_kind: TargetKind::WhiteboxTest,
    }));
}

#[test]
fn resolution_only_solves_requested_backends() {
    let (_dir, declarations) = discover(&[(
        "app",
        r#"
#cfg(target = "js")
import { "example/conditional/missing" }
#cfg(false)
import { "example/conditional/never" }
"#,
    )]);
    let log = UserLog::new(log::LevelFilter::Error);
    let resolved = declarations
        .clone()
        .resolve(&[Native, Native], false, &log)
        .unwrap();
    assert_eq!(
        resolved.pkg_rel.keys().copied().collect::<Vec<_>>(),
        [Native]
    );
    assert!(imports(&resolved, "app", TargetKind::Source, Native).is_empty());

    // A failure in any requested backend fails resolution, in either ordering.
    for backends in [&[Js][..], &[Native, Js], &[Js, Native]] {
        let error = declarations
            .clone()
            .resolve(backends, false, &log)
            .unwrap_err();
        assert!(matches!(error, ResolveError::SolveError(source)
            if matches!(*source, moonbuild_rupes_recta::pkg_solve::SolveError::ImportNotFound { ref import, .. }
                if import == "example/conditional/missing")));
    }
}

#[test]
fn disjoint_conditional_import_cycles_are_valid() {
    let (_dir, resolved) = resolve(&[
        (
            "a",
            r#"#cfg(target = "native") import { "example/conditional/b" }"#,
        ),
        (
            "b",
            r#"#cfg(target = "js") import { "example/conditional/a" }"#,
        ),
    ]);
    assert_eq!(
        imports(&resolved, "a", TargetKind::Source, Native),
        [("b".into(), "b".into())]
    );
    assert!(imports(&resolved, "b", TargetKind::Source, Native).is_empty());
    assert!(imports(&resolved, "a", TargetKind::Source, Js).is_empty());
    assert_eq!(
        imports(&resolved, "b", TargetKind::Source, Js),
        [("a".into(), "a".into())]
    );
}

#[test]
fn overlapping_conditional_imports_still_report_errors() {
    let log = UserLog::new(log::LevelFilter::Error);
    for (app, other) in [
        (
            r#"#cfg(target = "native") import { "example/conditional/other" @same, "example/conditional/third" @same }"#,
            "",
        ),
        (
            r#"#cfg(target = "native") import { "example/conditional/other" }"#,
            r#"#cfg(target = "native") import { "example/conditional/app" }"#,
        ),
    ] {
        let (_dir, declarations) = discover(&[("app", app), ("other", other), ("third", "")]);
        assert!(
            declarations
                .clone()
                .resolve(&[Native], false, &log)
                .is_err()
        );
        assert!(declarations.resolve(&[Js], false, &log).is_ok());
    }
}

#[test]
fn each_backend_resolves_virtual_implementations_and_overrides() {
    let (_dir, declarations) = discover(&[
        ("contract", r#"options("virtual": { "has-default": true })"#),
        (
            "implementation",
            r#"options("implement": "example/conditional/contract")"#,
        ),
        (
            "app",
            r#"
#cfg(target = "native") import { "example/conditional/contract" }
options("overrides": ["example/conditional/implementation"])
"#,
        ),
    ]);
    let resolved = declarations
        .resolve(&[Native, Js], false, &UserLog::new(log::LevelFilter::Error))
        .unwrap();
    let app = target(&resolved, "app", TargetKind::Source);
    let contract = target(&resolved, "contract", TargetKind::Source);
    let implementation = target(&resolved, "implementation", TargetKind::Source).package;
    for backend in [Native, Js] {
        let relation = &resolved.pkg_rel[&backend];
        assert_eq!(relation.virt_impl[implementation], contract.package);
        assert_eq!(
            relation.virtual_users[app.package].overrides[contract.package],
            implementation
        );
        assert_eq!(
            relation.dep_graph.contains_edge(app, contract),
            backend == Native
        );
    }
}
