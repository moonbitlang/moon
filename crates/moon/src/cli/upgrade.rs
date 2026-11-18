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

mod installer;

use anyhow::{Result, bail};

use super::UniversalFlags;

#[derive(Debug, clap::Parser, Clone)]
pub(super) struct UpgradeSubcommand {
    /// Force upgrade
    #[clap(long, short)]
    force: bool,

    /// Install the latest development version
    #[clap(long)]
    dev: bool,

    #[clap(long, hide = true)]
    non_interactive: bool,

    #[clap(long, hide = true)]
    base_url: Option<String>,
}

pub(super) fn run_upgrade(cli: UniversalFlags, cmd: UpgradeSubcommand) -> Result<i32> {
    if cli.dry_run {
        bail!("dry-run is not supported for upgrade")
    }
    installer::upgrade(cmd)
}
