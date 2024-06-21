use std::path::PathBuf;

use anyhow::bail;

use super::UniversalFlags;

/// Generate build matrix for benchmarking (legacy feature)
#[derive(Debug, clap::Parser)]
pub struct GenerateBuildMatrix {
    /// Set all of `drow`, `dcol`, `mrow`, `mcol` to the same value
    #[clap(short = 'n')]
    pub number: Option<u32>,

    /// Number of directory rows
    #[clap(long, long = "drow")]
    pub dir_rows: Option<u32>,

    /// Number of directory columns
    #[clap(long, long = "dcol")]
    pub dir_cols: Option<u32>,

    /// Number of module rows
    #[clap(long, long = "mrow")]
    pub mod_rows: Option<u32>,

    /// Number of module columns
    #[clap(long, long = "mcol")]
    pub mod_cols: Option<u32>,

    #[clap(long, long = "output-dir", short, short = 'o')]
    pub out_dir: PathBuf,
}

pub fn generate_build_matrix(
    _cli: &UniversalFlags,
    cmd: GenerateBuildMatrix,
) -> anyhow::Result<i32> {
    if _cli.dry_run {
        bail!("dry-run is not implemented for bench")
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
