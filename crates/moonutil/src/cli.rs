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

use tracing::warn;

// #[derive(clap::Parser)]
// pub struct StdInfo {
//     #[arg(long)]
//     std: bool,
//     #[arg(long, default = "false")]
//     no_std: bool,
// }

#[derive(Debug, clap::Parser, Serialize, Deserialize, Clone)]
#[clap(next_help_heading = "Common Options")]
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
    // The module `n2::trace` doesn't suppose parallelism now, so `--trace`
    // should be used in conjunction with `--serial` and `--no-parallelize`.
    #[clap(long, global = true)]
    pub trace: bool,

    /// Do not actually run the command
    #[clap(long, global = true)]
    pub dry_run: bool,

    #[clap(long, global = true, conflicts_with = "dry_run", hide = true)]
    pub build_graph: bool,

    /// Unstable flags to MoonBuild.
    #[clap(long, short = 'Z', default_value = "", env = "MOON_UNSTABLE")]
    pub unstable_feature: Box<crate::features::FeatureGate>,
}

impl UniversalFlags {
    /// Emit deprecation warnings for deprecated flags
    pub fn check_deprecations(&self) {
        if self.build_graph && self.unstable_feature.rupes_recta {
            warn!(
                "`--build-graph` is deprecated. Use -Z rr_export_module_graph, -Z rr_export_package_graph, or -Z rr_export_build_plan instead"
            );
        }

        if self.source_tgt_dir.source_dir.is_some() {
            // TODO(#1411): `--source-dir` used to be a hidden alias of `--directory`. We keep it
            // temporarily with the old meaning but warn because it was never a documented option.
            warn!(
                "`--source-dir` is a legacy/internal flag (not shown in help). It only affects project discovery (moon.mod.json lookup) and does not change the working directory."
            );
        }

        if self.source_tgt_dir.directory.is_some() {
            // TODO(#1411): `-C/--directory` will flip to real chdir semantics in a
            // future breaking release. For now we keep the historical meaning but warn.
            warn!(
                "`-C/--directory` is deprecated. It only affects project discovery (moon.mod.json lookup) and does not change the working directory; use `--cwd` if you intended to change the working directory."
            );
        }
    }
}
