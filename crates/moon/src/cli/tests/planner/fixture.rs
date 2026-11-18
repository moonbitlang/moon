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

use crate::cli::UniversalFlags;
use clap::Parser;
use moonbuild::execution::BuildInput;
use moonbuild::{BuildMeta, compile::BuildSession};
use std::{path::PathBuf, sync::Arc};

use moonbuild_rupes_recta::{
    DiscoveredProject, ResolvedProject, build_plan::ArtifactKey, model::PackageId,
};
use moonutil::{
    cond_expr::OptLevel,
    manifest::read_module_desc_file_in_dir,
    project::{SourceTargetDirs, WorkspaceEnv},
    resolution::{DirSyncResult, ModuleDependencyGraph, ModuleSource},
    target::TargetBackend,
    user_log::UserLog,
};

use crate::cli::{
    BenchSubcommand, BuildSubcommand, BundleSubcommand, CheckSubcommand, MoonBuildCli,
    MoonBuildSubcommands, RunSubcommand, TestSubcommand, test::TestLikeSubcommand,
};

pub(super) struct PlanningFixture {
    source_dir: PathBuf,
    target_dir: PathBuf,
    discovered: DiscoveredProject,
}

pub(super) struct PlannedGraph {
    build_meta: BuildMeta,
    build_graph: BuildInput,
}

/// The command and declared file dependencies in a dumped execution action.
#[derive(Debug, serde::Deserialize)]
pub(super) struct GraphAction {
    pub(super) command: String,
    pub(super) inputs: Vec<String>,
    pub(super) outputs: Vec<String>,
}

pub(super) fn graph_actions(graph: &str) -> Vec<GraphAction> {
    graph
        .lines()
        .map(|line| serde_json::from_str(line).expect("planner dump action should be valid JSON"))
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct PlannedPackageRun {
    pub(super) target_backend: TargetBackend,
    pub(super) packages: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct PlannedPackageIntent {
    pub(super) target_backend: TargetBackend,
    pub(super) profile: OptLevel,
    pub(super) packages: Vec<String>,
}

impl PlannedGraph {
    fn new(build_meta: BuildMeta, build_graph: BuildInput) -> Self {
        Self {
            build_meta,
            build_graph,
        }
    }
}

impl PlanningFixture {
    /// Plan a non-core module with no declared module dependencies against fixed core.
    /// Module acquisition and SDK compatibility belong to separate tests; package
    /// discovery, import resolution, and lowering still use their production paths.
    pub(super) fn with_core_dependency(case: &str) -> anyhow::Result<Self> {
        let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let source_dir = dunce::canonicalize(crate_root.join("tests/test_cases").join(case))?;
        let module = read_module_desc_file_in_dir(&source_dir)?;
        anyhow::ensure!(
            module.name != moonutil::constants::MOONBITLANG_CORE,
            "use PlanningFixture::new to plan core as a project"
        );
        anyhow::ensure!(
            module.deps.is_empty(),
            "core-dependency planning fixtures must not require module dependency resolution"
        );
        let (mut module_graph, root_id) = ModuleDependencyGraph::only_one_module(
            ModuleSource::from_local_module(&module, &source_dir)?,
            module,
        );
        let core_dir =
            dunce::canonicalize(crate_root.join("src/cli/tests/planner/fixtures/core_dependency"))?;
        let core = read_module_desc_file_in_dir(&core_dir)?;
        let core_id =
            module_graph.add_module(ModuleSource::from_stdlib(&core, &core_dir)?, Arc::new(core));
        module_graph.register_stdlib(core_id);
        let mut module_dirs = DirSyncResult::default();
        module_dirs.insert(root_id, source_dir.clone());
        module_dirs.insert(core_id, core_dir);
        let discovery = moonbuild_rupes_recta::DiscoveryConfig::default();
        let discovered = moonbuild_rupes_recta::discover_synced_project(
            &discovery,
            (module_graph, module_dirs),
            &UserLog::new(log::LevelFilter::Error),
        )?;
        Ok(Self {
            target_dir: source_dir.join("_build"),
            source_dir,
            discovered,
        })
    }

    pub(super) fn new(case: &str) -> anyhow::Result<Self> {
        let case_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases");
        // These planner tests only inspect graph construction, so they can use
        // the checked-in fixture directly without copying it to a temp directory.
        let fixture_dir = dunce::canonicalize(case_root.join(case))?;
        let user_log = UserLog::new(log::LevelFilter::Error);
        let dirs = SourceTargetDirs {
            cwd: None,
            target_dir: None,
        }
        .query_from(&fixture_dir, WorkspaceEnv::Auto)?
        .select(&user_log)?
        .package_dirs()?;
        let preparation_config =
            moonbuild::prepare::ProjectPreparationConfig::new_with_load_defaults(
                true,
                false,
                WorkspaceEnv::Auto,
            );
        let synced_modules =
            moonbuild::prepare::sync_module_dependencies(&preparation_config, &dirs, &user_log)?;
        let discovered = moonbuild_rupes_recta::discover_synced_project(
            &preparation_config.discovery,
            synced_modules,
            &user_log,
        )?;
        let source_dir = dirs.source_dir;
        let target_dir = dirs.target_dir;
        Ok(Self {
            source_dir,
            target_dir,
            discovered,
        })
    }

    /// Select one backend before resolution, as production project-wide plans do.
    pub(super) fn select_and_resolve(
        &self,
        selected_target_backend: Option<TargetBackend>,
        user_log: &UserLog,
    ) -> anyhow::Result<(TargetBackend, ResolvedProject)> {
        let target_backend = selected_target_backend.unwrap_or_else(|| {
            crate::cli::selection::local_modules_preferred_target(&self.discovered, user_log)
                .unwrap_or_default()
        });
        let resolved_project = self
            .discovered
            .clone()
            .resolve_packages(&[target_backend], user_log)?;
        Ok((target_backend, resolved_project))
    }

    pub(super) fn plan_test_with_cli(
        &self,
        cli: &UniversalFlags,
        cmd: &TestSubcommand,
    ) -> anyhow::Result<String> {
        let [plan]: [PlannedGraph; 1] =
            self.plan_test_all_with_cli(cli, cmd)?
                .try_into()
                .map_err(|plans: Vec<_>| {
                    anyhow::anyhow!("expected one test plan, got {}", plans.len())
                })?;
        self.dump_plan(plan)
    }

    pub(super) fn plan_test_all_with_cli(
        &self,
        cli: &UniversalFlags,
        cmd: &TestSubcommand,
    ) -> anyhow::Result<Vec<PlannedGraph>> {
        self.plan_test_all_with_backend(cli, cmd, cmd.build_flags.resolve_single_target_backend()?)
    }

    pub(super) fn plan_test_all_with_backend(
        &self,
        cli: &UniversalFlags,
        cmd: &TestSubcommand,
        selected_target_backend: Option<moonutil::target::TargetBackend>,
    ) -> anyhow::Result<Vec<PlannedGraph>> {
        let user_log = UserLog::new(cli.user_log_level());
        let session = BuildSession::new(&self.target_dir, true, &user_log)?;
        let borrowed: TestLikeSubcommand<'_> = cmd.into();
        crate::cli::test::plan_test_or_bench_rr_from_discovered(
            &session,
            cli,
            &borrowed,
            &self.target_dir,
            selected_target_backend.as_slice(),
            self.discovered.clone(),
            &user_log,
        )
        .map(|plans| {
            plans
                .into_iter()
                .map(|(meta, graph, _filter)| PlannedGraph::new(meta, graph))
                .collect()
        })
    }

    pub(super) fn plan_bench_with_cli(
        &self,
        cli: &UniversalFlags,
        cmd: &BenchSubcommand,
    ) -> anyhow::Result<String> {
        let [plan]: [PlannedGraph; 1] = self
            .plan_bench_all_with_cli(cli, cmd)?
            .try_into()
            .map_err(|plans: Vec<_>| {
                anyhow::anyhow!("expected one bench plan, got {}", plans.len())
            })?;
        self.dump_plan(plan)
    }

    pub(super) fn plan_bench_all_with_cli(
        &self,
        cli: &UniversalFlags,
        cmd: &BenchSubcommand,
    ) -> anyhow::Result<Vec<PlannedGraph>> {
        let user_log = UserLog::new(cli.user_log_level());
        let session = BuildSession::new(&self.target_dir, true, &user_log)?;
        let borrowed: TestLikeSubcommand<'_> = cmd.into();
        crate::cli::test::plan_test_or_bench_rr_from_discovered(
            &session,
            cli,
            &borrowed,
            &self.target_dir,
            cmd.build_flags.resolve_single_target_backend()?.as_slice(),
            self.discovered.clone(),
            &user_log,
        )
        .map(|plans| {
            plans
                .into_iter()
                .map(|(meta, graph, _filter)| PlannedGraph::new(meta, graph))
                .collect()
        })
    }

    pub(super) fn plan_build_with_cli(
        &self,
        cli: &UniversalFlags,
        cmd: &BuildSubcommand,
    ) -> anyhow::Result<String> {
        self.dump_plan(self.plan_build_graph_with_cli(cli, cmd)?)
    }

    pub(super) fn plan_build_graph_with_cli(
        &self,
        cli: &UniversalFlags,
        cmd: &BuildSubcommand,
    ) -> anyhow::Result<PlannedGraph> {
        let mut plans = self.plan_build_all_with_cli(cli, cmd)?;
        anyhow::ensure!(
            plans.len() == 1,
            "expected one backend plan, got {}",
            plans.len()
        );
        Ok(plans.pop().expect("checked exactly one plan"))
    }

    pub(super) fn plan_build_all_with_cli(
        &self,
        cli: &UniversalFlags,
        cmd: &BuildSubcommand,
    ) -> anyhow::Result<Vec<PlannedGraph>> {
        self.plan_build_all_with_backend(cli, cmd, cmd.build_flags.resolve_single_target_backend()?)
    }

    pub(super) fn plan_build_all_with_backend(
        &self,
        cli: &UniversalFlags,
        cmd: &BuildSubcommand,
        selected_target_backend: Option<moonutil::target::TargetBackend>,
    ) -> anyhow::Result<Vec<PlannedGraph>> {
        let user_log = UserLog::new(cli.user_log_level());
        let session = BuildSession::new(&self.target_dir, true, &user_log)?;
        crate::cli::build::plan_build_rr_from_discovered(
            &session,
            cli,
            cmd,
            &self.target_dir,
            selected_target_backend.as_slice(),
            self.discovered.clone(),
            &user_log,
        )
        .map(|plans| {
            plans
                .into_iter()
                .map(|(meta, graph)| PlannedGraph::new(meta, graph))
                .collect()
        })
    }

    pub(super) fn plan_bundle_all_with_backend(
        &self,
        cli: &UniversalFlags,
        cmd: &BundleSubcommand,
        selected_target_backend: Option<moonutil::target::TargetBackend>,
    ) -> anyhow::Result<Vec<PlannedGraph>> {
        let user_log = UserLog::new(cli.user_log_level());
        let session = BuildSession::new(&self.target_dir, true, &user_log)?;
        crate::cli::bundle::plan_bundle_rr_from_discovered(
            &session,
            cli,
            cmd,
            &self.target_dir,
            selected_target_backend.as_slice(),
            self.discovered.clone(),
            &user_log,
        )
        .map(|plans| {
            plans
                .into_iter()
                .map(|(meta, graph)| PlannedGraph::new(meta, graph))
                .collect()
        })
    }

    pub(super) fn plan_check_with_cli(
        &self,
        cli: &UniversalFlags,
        cmd: &CheckSubcommand,
    ) -> anyhow::Result<String> {
        let mut plans = self.plan_check_all_with_cli(cli, cmd)?;
        assert_eq!(plans.len(), 1, "expected a single-backend check plan");
        self.dump_plan(plans.pop().unwrap())
    }

    pub(super) fn plan_check_all_with_cli(
        &self,
        cli: &UniversalFlags,
        cmd: &CheckSubcommand,
    ) -> anyhow::Result<Vec<PlannedGraph>> {
        self.plan_check_all_with_backend(cli, cmd, cmd.build_flags.resolve_single_target_backend()?)
    }

    pub(super) fn plan_check_all_with_backend(
        &self,
        cli: &UniversalFlags,
        cmd: &CheckSubcommand,
        selected_target_backend: Option<moonutil::target::TargetBackend>,
    ) -> anyhow::Result<Vec<PlannedGraph>> {
        let user_log = UserLog::new(cli.user_log_level());
        let session = BuildSession::new(&self.target_dir, true, &user_log)?;
        crate::cli::check::plan_check_rr_from_discovered(
            &session,
            cli,
            cmd,
            &self.source_dir,
            &self.target_dir,
            selected_target_backend.as_slice(),
            self.discovered.clone(),
            &user_log,
        )
        .map(|plans| {
            plans
                .into_iter()
                .map(|(meta, graph)| PlannedGraph::new(meta, graph))
                .collect()
        })
    }

    pub(super) fn plan_run_with_cli(
        &self,
        cli: &UniversalFlags,
        cmd: &RunSubcommand,
    ) -> anyhow::Result<String> {
        self.dump_plan(self.plan_run_graph_with_cli(cli, cmd)?)
    }

    pub(super) fn plan_run_graph_with_cli(
        &self,
        cli: &UniversalFlags,
        cmd: &RunSubcommand,
    ) -> anyhow::Result<PlannedGraph> {
        let user_log = UserLog::new(cli.user_log_level());
        let session = BuildSession::new(&self.target_dir, true, &user_log)?;
        let mut cmd = cmd.clone();
        if cmd.command.is_none()
            && let Some(input) = cmd.package_or_mbt_file.as_deref()
        {
            let input_path = PathBuf::from(input);
            if input_path.is_relative() {
                cmd.package_or_mbt_file = Some(self.source_dir.join(input).display().to_string());
            }
        }
        let (build_meta, build_graph) = crate::cli::run::plan_run_rr_from_discovered(
            &session,
            cli,
            &cmd,
            &self.target_dir,
            cmd.build_flags.resolve_single_target_backend()?,
            self.discovered.clone(),
            &user_log,
        )?;
        Ok(PlannedGraph::new(build_meta, build_graph))
    }

    pub(super) fn case_dir(&self) -> &std::path::Path {
        &self.source_dir
    }

    fn dump_plan(&self, plan: PlannedGraph) -> anyhow::Result<String> {
        let mut out = Vec::new();
        crate::dry_run::write_build_graph(
            &mut out,
            &plan.build_graph,
            plan.build_meta
                .artifacts
                .values()
                .flatten()
                .map(PathBuf::as_path),
            &self.source_dir,
        )?;
        Ok(String::from_utf8(out).expect("graph dump should be valid UTF-8"))
    }
}

pub(super) fn planned_root_package_runs(runs: Vec<PlannedGraph>) -> Vec<PlannedPackageRun> {
    runs.into_iter()
        .map(|plan| PlannedPackageRun {
            target_backend: plan.build_meta.target_backend(),
            packages: root_package_names(&plan.build_meta),
        })
        .collect()
}

pub(super) fn planned_check_package_runs(runs: Vec<PlannedGraph>) -> Vec<PlannedPackageRun> {
    runs.into_iter()
        .map(|plan| PlannedPackageRun {
            target_backend: plan.build_meta.target_backend(),
            packages: check_package_names(&plan.build_meta),
        })
        .collect()
}

pub(super) fn planned_target_backends(runs: Vec<PlannedGraph>) -> Vec<TargetBackend> {
    runs.into_iter()
        .map(|plan| plan.build_meta.target_backend())
        .collect()
}

pub(super) fn planned_root_package_intent(plan: PlannedGraph) -> PlannedPackageIntent {
    PlannedPackageIntent {
        target_backend: plan.build_meta.target_backend(),
        profile: plan.build_meta.opt_level,
        packages: root_package_names(&plan.build_meta),
    }
}

/// The requested artifacts of a plan as `{package} {artifact kind}` labels.
pub(super) fn planned_requested_artifacts(plan: PlannedGraph) -> Vec<String> {
    let meta = plan.build_meta;
    meta.artifacts
        .keys()
        .filter_map(|artifact| {
            let (package, kind) = match artifact {
                ArtifactKey::CoreIr { package, .. } => (package, "core"),
                ArtifactKey::Executable { package, .. } => (package, "executable"),
                ArtifactKey::ForeignLibrary { package } => (package, "foreign library"),
                _ => return None,
            };
            let fqn = &meta.resolved_project.pkg_dirs.get_package(*package).fqn;
            Some(format!("{fqn} {kind}"))
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn planned_graph_inputs(graph: &str) -> std::collections::BTreeSet<String> {
    graph
        .lines()
        .flat_map(|line| {
            let line_json: serde_json::Value =
                serde_json::from_str(line).expect("planner dump line should be valid JSON");
            line_json["inputs"]
                .as_array()
                .expect("planner dump line should contain inputs")
                .iter()
                .map(|input| {
                    input
                        .as_str()
                        .expect("planner input path should be a string")
                        .to_string()
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn root_package_names(meta: &BuildMeta) -> Vec<String> {
    package_names(meta, |artifact| match artifact {
        ArtifactKey::CheckMi { package, .. }
        | ArtifactKey::BuildMi { package, .. }
        | ArtifactKey::CoreIr { package, .. }
        | ArtifactKey::ProofMi { package, .. }
        | ArtifactKey::ProofWhyml { package, .. }
        | ArtifactKey::LinkedCore { package, .. }
        | ArtifactKey::Executable { package, .. }
        | ArtifactKey::ForeignLibrary { package }
        | ArtifactKey::GeneratedTestDriver { package, .. }
        | ArtifactKey::GeneratedTestMetadata { package, .. }
        | ArtifactKey::GeneratedMbti { package, .. } => Some(*package),
        _ => None,
    })
}

fn check_package_names(meta: &BuildMeta) -> Vec<String> {
    package_names(meta, |artifact| match artifact {
        ArtifactKey::CheckMi { package, .. } => Some(*package),
        _ => None,
    })
}

fn package_names(
    meta: &BuildMeta,
    package_of: impl Fn(&ArtifactKey) -> Option<PackageId>,
) -> Vec<String> {
    meta.artifacts
        .keys()
        .filter_map(package_of)
        .map(|pkg_id| {
            meta.resolved_project
                .pkg_dirs
                .get_package(pkg_id)
                .fqn
                .to_string()
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn parse_build_command(args: &[&str]) -> (UniversalFlags, BuildSubcommand) {
    let parsed = MoonBuildCli::try_parse_from(std::iter::once("moon").chain(args.iter().copied()))
        .expect("build command should parse");
    let Some(MoonBuildSubcommands::Build(cmd)) = parsed.subcommand else {
        panic!("expected `moon build` to parse as the build subcommand");
    };
    (parsed.flags, cmd)
}

pub(super) fn parse_check_command(args: &[&str]) -> (UniversalFlags, CheckSubcommand) {
    let parsed = MoonBuildCli::try_parse_from(std::iter::once("moon").chain(args.iter().copied()))
        .expect("check command should parse");
    let Some(MoonBuildSubcommands::Check(cmd)) = parsed.subcommand else {
        panic!("expected `moon check` to parse as the check subcommand");
    };
    (parsed.flags, cmd)
}

pub(super) fn parse_bundle_command(args: &[&str]) -> (UniversalFlags, BundleSubcommand) {
    let parsed = MoonBuildCli::try_parse_from(std::iter::once("moon").chain(args.iter().copied()))
        .expect("bundle command should parse");
    let Some(MoonBuildSubcommands::Bundle(cmd)) = parsed.subcommand else {
        panic!("expected `moon bundle` to parse as the bundle subcommand");
    };
    (parsed.flags, cmd)
}

pub(super) fn parse_run_command(args: &[&str]) -> (UniversalFlags, RunSubcommand) {
    let parsed = MoonBuildCli::try_parse_from(std::iter::once("moon").chain(args.iter().copied()))
        .expect("run command should parse");
    let Some(MoonBuildSubcommands::Run(cmd)) = parsed.subcommand else {
        panic!("expected `moon run` to parse as the run subcommand");
    };
    (parsed.flags, cmd)
}

pub(super) fn parse_test_command(args: &[&str]) -> (UniversalFlags, TestSubcommand) {
    let parsed = MoonBuildCli::try_parse_from(std::iter::once("moon").chain(args.iter().copied()))
        .expect("test command should parse");
    let Some(MoonBuildSubcommands::Test(cmd)) = parsed.subcommand else {
        panic!("expected `moon test` to parse as the test subcommand");
    };
    (parsed.flags, cmd)
}

pub(super) fn parse_bench_command(args: &[&str]) -> (UniversalFlags, BenchSubcommand) {
    let parsed = MoonBuildCli::try_parse_from(std::iter::once("moon").chain(args.iter().copied()))
        .expect("bench command should parse");
    let Some(MoonBuildSubcommands::Bench(cmd)) = parsed.subcommand else {
        panic!("expected `moon bench` to parse as the bench subcommand");
    };
    (parsed.flags, cmd)
}
