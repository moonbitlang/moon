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

use serde::{Deserialize, Serialize};

use crate::dirs::{SourceTargetDirs, WorkspaceEnv};

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

    #[clap(skip)]
    #[serde(skip, default)]
    pub workspace_env: WorkspaceEnv,

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

    /// Unstable flags to MoonBuild.
    #[clap(long, short = 'Z', default_value = "", env = "MOON_UNSTABLE")]
    pub unstable_feature: Box<crate::features::FeatureGate>,
}

impl UniversalFlags {
    pub fn user_log_level(&self) -> log::LevelFilter {
        crate::user_log::user_log_level(self.verbose, self.quiet)
    }
}

pub fn dialoguer_ctrlc_handler() {
    // Fix cursor disappears after ctrc+c
    // https://github.com/console-rs/dialoguer/issues/77
    let term = dialoguer::console::Term::stdout();
    let _ = term.show_cursor();
    std::process::exit(1);
}
