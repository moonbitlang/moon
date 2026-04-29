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

use clap::Parser;
use moonbuild_debug::graph::debug_dump_build_graph;
use std::path::PathBuf;

use moonbuild_rupes_recta::ResolveOutput;
use moonutil::{cli::UniversalFlags, common::BUILD_DIR, dirs::WorkspaceEnv};

use crate::cli::{
    BenchSubcommand, BuildSubcommand, CheckSubcommand, MoonBuildCli, MoonBuildSubcommands,
    RunSubcommand, TestLikeSubcommand, TestSubcommand,
};

pub(super) struct PlanningFixture {
    source_dir: PathBuf,
    target_dir: PathBuf,
    resolve_output: ResolveOutput,
}

impl PlanningFixture {
    pub(super) fn new(case: &str) -> anyhow::Result<Self> {
        let case_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases");
        // These planner tests only inspect graph construction, so they can use
        // the checked-in fixture directly without copying it to a temp directory.
        let source_dir = dunce::canonicalize(case_root.join(case))?;
        let target_dir = source_dir.join(BUILD_DIR);
        let mooncakes_dir = source_dir.join(".mooncakes");
        let resolve_cfg = moonbuild_rupes_recta::ResolveConfig::new_with_load_defaults(
            true,
            false,
            false,
            WorkspaceEnv::Auto,
        );
        let resolve_output =
            moonbuild_rupes_recta::resolve(&resolve_cfg, &source_dir, &mooncakes_dir)?;
        Ok(Self {
            source_dir,
            target_dir,
            resolve_output,
        })
    }

    pub(super) fn plan_test_with_cli(
        &self,
        cli: &UniversalFlags,
        cmd: &TestSubcommand,
    ) -> anyhow::Result<String> {
        let borrowed: TestLikeSubcommand<'_> = cmd.into();
        let (build_meta, build_graph, _) = crate::cli::test::plan_test_or_bench_rr_from_resolved(
            cli,
            &borrowed,
            &self.target_dir,
            cmd.build_flags.resolve_single_target_backend()?,
            self.resolve_output.clone(),
        )?;
        self.dump_plan(build_meta, build_graph)
    }

    pub(super) fn plan_test_all_with_cli(
        &self,
        cli: &UniversalFlags,
        cmd: &TestSubcommand,
    ) -> anyhow::Result<Vec<(crate::rr_build::BuildMeta, crate::rr_build::BuildInput)>> {
        let borrowed: TestLikeSubcommand<'_> = cmd.into();
        crate::cli::test::plan_test_or_bench_rr_from_resolved_all(
            cli,
            &borrowed,
            &self.target_dir,
            cmd.build_flags.resolve_single_target_backend()?,
            self.resolve_output.clone(),
        )
        .map(|plans| {
            plans
                .into_iter()
                .map(|(meta, graph, _filter)| (meta, graph))
                .collect()
        })
    }

    pub(super) fn plan_bench_with_cli(
        &self,
        cli: &UniversalFlags,
        cmd: &BenchSubcommand,
    ) -> anyhow::Result<String> {
        let borrowed: TestLikeSubcommand<'_> = cmd.into();
        let (build_meta, build_graph, _) = crate::cli::test::plan_test_or_bench_rr_from_resolved(
            cli,
            &borrowed,
            &self.target_dir,
            cmd.build_flags.resolve_single_target_backend()?,
            self.resolve_output.clone(),
        )?;
        self.dump_plan(build_meta, build_graph)
    }

    pub(super) fn plan_bench_all_with_cli(
        &self,
        cli: &UniversalFlags,
        cmd: &BenchSubcommand,
    ) -> anyhow::Result<Vec<(crate::rr_build::BuildMeta, crate::rr_build::BuildInput)>> {
        let borrowed: TestLikeSubcommand<'_> = cmd.into();
        crate::cli::test::plan_test_or_bench_rr_from_resolved_all(
            cli,
            &borrowed,
            &self.target_dir,
            cmd.build_flags.resolve_single_target_backend()?,
            self.resolve_output.clone(),
        )
        .map(|plans| {
            plans
                .into_iter()
                .map(|(meta, graph, _filter)| (meta, graph))
                .collect()
        })
    }

    pub(super) fn plan_build_with_cli(
        &self,
        cli: &UniversalFlags,
        cmd: &BuildSubcommand,
    ) -> anyhow::Result<String> {
        let (build_meta, build_graph) = crate::cli::build::plan_build_rr_from_resolved(
            cli,
            cmd,
            &self.target_dir,
            cmd.build_flags.resolve_single_target_backend()?,
            self.resolve_output.clone(),
        )?;
        self.dump_plan(build_meta, build_graph)
    }

    pub(super) fn plan_build_all_with_cli(
        &self,
        cli: &UniversalFlags,
        cmd: &BuildSubcommand,
    ) -> anyhow::Result<Vec<(crate::rr_build::BuildMeta, crate::rr_build::BuildInput)>> {
        crate::cli::build::plan_build_rr_from_resolved_all(
            cli,
            cmd,
            &self.source_dir,
            &self.target_dir,
            cmd.build_flags.resolve_single_target_backend()?,
            self.resolve_output.clone(),
        )
    }

    pub(super) fn plan_check_with_cli(
        &self,
        cli: &UniversalFlags,
        cmd: &CheckSubcommand,
    ) -> anyhow::Result<String> {
        let (build_meta, build_graph) = crate::cli::check::plan_check_rr_from_resolved(
            cli,
            cmd,
            &self.source_dir,
            &self.target_dir,
            cmd.build_flags.resolve_single_target_backend()?,
            self.resolve_output.clone(),
        )?;
        self.dump_plan(build_meta, build_graph)
    }

    pub(super) fn plan_check_all_with_cli(
        &self,
        cli: &UniversalFlags,
        cmd: &CheckSubcommand,
    ) -> anyhow::Result<Vec<(crate::rr_build::BuildMeta, crate::rr_build::BuildInput)>> {
        crate::cli::check::plan_check_rr_from_resolved_all(
            cli,
            cmd,
            &self.source_dir,
            &self.target_dir,
            cmd.build_flags.resolve_single_target_backend()?,
            self.resolve_output.clone(),
        )
    }

    pub(super) fn plan_run_with_cli(
        &self,
        cli: &UniversalFlags,
        cmd: &RunSubcommand,
    ) -> anyhow::Result<String> {
        let mut cmd = cmd.clone();
        if cmd.command.is_none()
            && let Some(input) = cmd.package_or_mbt_file.as_deref()
        {
            let input_path = PathBuf::from(input);
            if input_path.is_relative() {
                cmd.package_or_mbt_file = Some(self.source_dir.join(input).display().to_string());
            }
        }
        let (build_meta, build_graph) = crate::cli::run::plan_run_rr_from_resolved(
            cli,
            &cmd,
            &self.source_dir,
            &self.target_dir,
            cmd.build_flags.resolve_single_target_backend()?,
            self.resolve_output.clone(),
        )?;
        self.dump_plan(build_meta, build_graph)
    }

    pub(super) fn case_dir(&self) -> &std::path::Path {
        &self.source_dir
    }

    fn dump_plan(
        &self,
        build_meta: crate::rr_build::BuildMeta,
        build_graph: crate::rr_build::BuildInput,
    ) -> anyhow::Result<String> {
        let graph = build_graph.graph_for_test();
        let default_files = build_meta
            .artifacts
            .values()
            .flat_map(|art| {
                art.artifacts
                    .iter()
                    .flat_map(|file| graph.files.lookup(&file.to_string_lossy()))
            })
            .collect::<Vec<_>>();
        let dump = debug_dump_build_graph(graph, &default_files, &self.source_dir);
        let mut out = Vec::new();
        dump.dump_to(&mut out).expect("graph dump should serialize");
        Ok(String::from_utf8(out).expect("graph dump should be valid UTF-8"))
    }
}

pub(super) fn parse_build_command(args: &[&str]) -> (UniversalFlags, BuildSubcommand) {
    let parsed = MoonBuildCli::try_parse_from(std::iter::once("moon").chain(args.iter().copied()))
        .expect("build command should parse");
    let MoonBuildSubcommands::Build(cmd) = parsed.subcommand else {
        panic!("expected `moon build` to parse as the build subcommand");
    };
    (parsed.flags, cmd)
}

pub(super) fn parse_check_command(args: &[&str]) -> (UniversalFlags, CheckSubcommand) {
    let parsed = MoonBuildCli::try_parse_from(std::iter::once("moon").chain(args.iter().copied()))
        .expect("check command should parse");
    let MoonBuildSubcommands::Check(cmd) = parsed.subcommand else {
        panic!("expected `moon check` to parse as the check subcommand");
    };
    (parsed.flags, cmd)
}

pub(super) fn parse_run_command(args: &[&str]) -> (UniversalFlags, RunSubcommand) {
    let parsed = MoonBuildCli::try_parse_from(std::iter::once("moon").chain(args.iter().copied()))
        .expect("run command should parse");
    let MoonBuildSubcommands::Run(cmd) = parsed.subcommand else {
        panic!("expected `moon run` to parse as the run subcommand");
    };
    (parsed.flags, cmd)
}

pub(super) fn parse_test_command(args: &[&str]) -> (UniversalFlags, TestSubcommand) {
    let parsed = MoonBuildCli::try_parse_from(std::iter::once("moon").chain(args.iter().copied()))
        .expect("test command should parse");
    let MoonBuildSubcommands::Test(cmd) = parsed.subcommand else {
        panic!("expected `moon test` to parse as the test subcommand");
    };
    (parsed.flags, cmd)
}

pub(super) fn parse_bench_command(args: &[&str]) -> (UniversalFlags, BenchSubcommand) {
    let parsed = MoonBuildCli::try_parse_from(std::iter::once("moon").chain(args.iter().copied()))
        .expect("bench command should parse");
    let MoonBuildSubcommands::Bench(cmd) = parsed.subcommand else {
        panic!("expected `moon bench` to parse as the bench subcommand");
    };
    (parsed.flags, cmd)
}
