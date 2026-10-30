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

use std::path::PathBuf;

use anyhow::bail;

use super::UniversalFlags;

/// Generate build matrix for benchmarking (legacy feature)
#[derive(Debug, clap::Parser)]
pub(crate) struct GenerateBuildMatrix {
    /// Set all of `drow`, `dcol`, `mrow`, `mcol` to the same value
    #[clap(short = 'n')]
    pub number: Option<u32>,

    /// Number of directory rows
    #[clap(long = "drow")]
    pub dir_rows: Option<u32>,

    /// Number of directory columns
    #[clap(long = "dcol")]
    pub dir_cols: Option<u32>,

    /// Number of module rows
    #[clap(long = "mrow")]
    pub mod_rows: Option<u32>,

    /// Number of module columns
    #[clap(long = "mcol")]
    pub mod_cols: Option<u32>,

    /// The output directory
    #[clap(long = "output-dir", short = 'o')]
    pub out_dir: PathBuf,
}

pub(crate) fn generate_build_matrix(
    _cli: &UniversalFlags,
    cmd: GenerateBuildMatrix,
) -> anyhow::Result<i32> {
    if _cli.dry_run {
        bail!("dry-run is not supported for bench")
    }

    let n = cmd.number.unwrap_or(1);
    let dir_rows = cmd.dir_rows.unwrap_or(n);
    let dir_cols = cmd.dir_cols.unwrap_or(n);
    let mod_rows = cmd.mod_rows.unwrap_or(n);
    let mod_cols = cmd.mod_cols.unwrap_or(n);

    let mut config = moonbuild::bench::Config::new();
    config.dir_rows = dir_rows;
    config.dir_cols = dir_cols;
    config.mod_rows = mod_rows;
    config.mod_cols = mod_cols;

    moonbuild::bench::write(&config, &cmd.out_dir);
    Ok(0)
}
