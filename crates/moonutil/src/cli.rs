use serde::{Deserialize, Serialize};

use crate::dirs::SourceTargetDirs;

// #[derive(clap::Parser)]
// pub struct StdInfo {
//     #[arg(long)]
//     std: bool,
//     #[arg(long, default = "false")]
//     no_std: bool,
// }

#[derive(Debug, clap::Parser, Serialize, Deserialize)]
#[clap(next_display_order(2000), next_help_heading("Common options"))]
pub struct UniversalFlags {
    #[clap(flatten)]
    pub source_tgt_dir: SourceTargetDirs,

    /// Suppress output
    #[clap(long, short = 'q', global = true)]
    pub quiet: bool,

    /// Increase verbosity
    #[clap(long, short = 'v', global = true)]
    pub verbose: bool,

    /// Trace the execution of the program
    #[clap(long, global = true)]
    pub trace: bool,

    /// Do not actually run the command
    #[clap(long, global = true)]
    pub dry_run: bool,
}
