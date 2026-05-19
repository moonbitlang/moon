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

use anyhow::Context;
use moonutil::{
    common::{TargetBackend, TestIndexRange, lower_surface_targets},
    dirs::PackageDirs,
    mooncakes::sync::AutoSyncFlags,
};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};
use tracing::{Level, instrument};

use super::{BuildFlags, UniversalFlags};

/// Run benchmarks in the current package
#[derive(Debug, clap::Parser)]
pub(crate) struct BenchSubcommand {
    #[clap(flatten)]
    pub build_flags: BuildFlags,

    /// Run test in the specified package
    #[clap(short, long, num_args(1..))]
    pub package: Option<Vec<String>>,

    /// Run test in the specified file. Only valid when `--package` is also specified.
    #[clap(short, long, requires("package"))]
    pub file: Option<String>,

    /// Run only the index-th test in the file. Accepts a single index or a left-inclusive
    /// right-exclusive range like `0-2`. Only valid when `--file` is also specified.
    #[clap(short, long, requires("file"))]
    pub index: Option<TestIndexRange>,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Only build, do not bench
    #[clap(long)]
    pub build_only: bool,

    /// Output benchmark results in JSON Lines format to the specified file
    #[clap(long, value_name = "JSONL", conflicts_with = "build_only")]
    pub jsonl: Option<PathBuf>,

    /// Run the benchmarks in a target backend sequentially
    #[clap(long)]
    pub no_parallelize: bool,
}

#[instrument(skip_all)]
pub(crate) fn run_bench(cli: UniversalFlags, cmd: BenchSubcommand) -> anyhow::Result<i32> {
    if cmd.jsonl.is_some() && cli.dry_run {
        anyhow::bail!("`--jsonl` cannot be used with `--dry-run`");
    }

    let mut jsonl_writer = cmd
        .jsonl
        .as_ref()
        .map(|path| {
            File::create(path)
                .map(BufWriter::new)
                .with_context(|| format!("failed to create JSON Lines file `{}`", path.display()))
        })
        .transpose()?;

    let PackageDirs {
        source_dir,
        target_dir,
        mooncakes_dir,
        project_manifest_path,
    } = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .package_dirs()?;

    let exit_code = if cmd.build_flags.target.is_empty() {
        let writer = jsonl_writer.as_mut().map(|writer| writer as &mut dyn Write);
        run_bench_internal(
            &cli,
            &cmd,
            &source_dir,
            &target_dir,
            &mooncakes_dir,
            project_manifest_path.as_deref(),
            None,
            None,
            writer,
        )?
    } else {
        let surface_targets = cmd.build_flags.target.clone();
        let targets = lower_surface_targets(&surface_targets);
        let display_backend_hint = if targets.len() > 1 { Some(()) } else { None };

        let mut ret_value = 0;
        for t in targets {
            let writer = jsonl_writer.as_mut().map(|writer| writer as &mut dyn Write);
            let x = run_bench_internal(
                &cli,
                &cmd,
                &source_dir,
                &target_dir,
                &mooncakes_dir,
                project_manifest_path.as_deref(),
                display_backend_hint,
                Some(t),
                writer,
            )
            .context(format!("failed to run bench for target {t:?}"))?;
            ret_value = ret_value.max(x);
        }
        ret_value
    };

    if let (Some(path), Some(writer)) = (cmd.jsonl.as_ref(), jsonl_writer.as_mut()) {
        writer
            .flush()
            .with_context(|| format!("failed to flush JSON Lines file `{}`", path.display()))?;
    }
    Ok(exit_code)
}

#[instrument(level = Level::DEBUG, skip_all)]
#[allow(clippy::too_many_arguments)]
fn run_bench_internal(
    cli: &UniversalFlags,
    cmd: &BenchSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    mooncakes_dir: &Path,
    project_manifest_path: Option<&Path>,
    display_backend_hint: Option<()>,
    selected_target_backend: Option<TargetBackend>,
    bench_jsonl_writer: Option<&mut dyn Write>,
) -> anyhow::Result<i32> {
    super::run_test_or_bench_internal(
        cli,
        cmd.into(),
        source_dir,
        target_dir,
        mooncakes_dir,
        project_manifest_path,
        display_backend_hint,
        selected_target_backend,
        bench_jsonl_writer,
    )
}
