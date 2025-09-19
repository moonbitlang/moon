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
use moonutil::{common::lower_surface_targets, dirs::PackageDirs, mooncakes::sync::AutoSyncFlags};
use std::path::Path;
use tracing::{instrument, Level};

use super::{BuildFlags, UniversalFlags};

/// Run benchmarks in the current package
#[derive(Debug, clap::Parser, Clone)]
pub struct BenchSubcommand {
    #[clap(flatten)]
    pub build_flags: BuildFlags,

    /// Run test in the specified package
    #[clap(short, long, num_args(0..))]
    pub package: Option<Vec<String>>,

    /// Run test in the specified file. Only valid when `--package` is also specified.
    #[clap(short, long, requires("package"))]
    pub file: Option<String>,

    /// Run only the index-th test in the file. Only valid when `--file` is also specified.
    #[clap(short, long, requires("file"))]
    pub index: Option<u32>,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Only build, do not bench
    #[clap(long)]
    pub build_only: bool,

    /// Run the benchmarks in a target backend sequentially
    #[clap(long)]
    pub no_parallelize: bool,
}

#[instrument(skip_all)]
pub fn run_bench(cli: UniversalFlags, cmd: BenchSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    if cmd.build_flags.target.is_none() {
        return run_bench_internal(&cli, &cmd, &source_dir, &target_dir, None);
    }
    let surface_targets = cmd.build_flags.target.clone().unwrap();
    let targets = lower_surface_targets(&surface_targets);
    let display_backend_hint = if targets.len() > 1 { Some(()) } else { None };

    let mut ret_value = 0;
    for t in targets {
        let mut cmd = cmd.clone();
        cmd.build_flags.target_backend = Some(t);
        let x = run_bench_internal(&cli, &cmd, &source_dir, &target_dir, display_backend_hint)
            .context(format!("failed to run bench for target {t:?}"))?;
        ret_value = ret_value.max(x);
    }
    Ok(ret_value)
}

#[instrument(level = Level::DEBUG, skip_all)]
fn run_bench_internal(
    cli: &UniversalFlags,
    cmd: &BenchSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    display_backend_hint: Option<()>,
) -> anyhow::Result<i32> {
    super::run_test_or_bench_internal(
        cli,
        cmd.into(),
        source_dir,
        target_dir,
        display_backend_hint,
    )
}
