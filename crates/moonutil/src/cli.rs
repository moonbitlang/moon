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

use serde::{Deserialize, Serialize};

use crate::dirs::SourceTargetDirs;

// #[derive(clap::Parser)]
// pub struct StdInfo {
//     #[arg(long)]
//     std: bool,
//     #[arg(long, default = "false")]
//     no_std: bool,
// }

#[derive(Debug, clap::Parser, Serialize, Deserialize, Clone)]
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

    /// generate build graph
    #[clap(long, global = true, conflicts_with = "dry_run")]
    pub build_graph: bool,
}
