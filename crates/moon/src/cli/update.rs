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

use anyhow::bail;
use mooncake::registry::RegistryClient;
use moonutil::user_log::UserLog;

use super::UniversalFlags;

/// Update the package registry index
#[derive(Debug, clap::Parser)]
pub(crate) struct UpdateSubcommand {}

pub(crate) fn update_cli(
    cli: UniversalFlags,
    _cmd: UpdateSubcommand,
    user_log: &UserLog,
) -> anyhow::Result<i32> {
    if cli.dry_run {
        bail!("dry-run is not supported for update")
    }
    RegistryClient::configured().sync(user_log)?;
    Ok(0)
}
