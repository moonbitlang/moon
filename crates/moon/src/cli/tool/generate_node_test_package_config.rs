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

use std::path::PathBuf;

use anyhow::bail;
use moonutil::cli_support::UniversalFlags;

#[derive(Debug, clap::Parser)]
pub(crate) struct GenerateNodeTestPackageConfigSubcommand {
    /// The output `package.json` file.
    #[clap(long)]
    output: PathBuf,
}

pub(crate) fn generate_node_test_package_config(
    cli: &UniversalFlags,
    cmd: GenerateNodeTestPackageConfigSubcommand,
) -> anyhow::Result<i32> {
    if cli.dry_run {
        bail!("dry-run is not supported for tool generate-node-test-package-config");
    }

    cmd.output
        .parent()
        .map(std::fs::create_dir_all)
        .transpose()?;
    // This is deliberately type-neutral. Its only purpose is to stop Node
    // from inheriting package settings from the user's project.
    std::fs::write(cmd.output, "{}")?;
    Ok(0)
}
