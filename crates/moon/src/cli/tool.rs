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

pub(crate) mod build_binary_dep;
pub(crate) mod embed;
pub(crate) mod format_and_diff;
pub(crate) mod write_rsp_file;

use embed::*;
use format_and_diff::*;
use moonutil::cli::UniversalFlags;
use write_rsp_file::*;

#[derive(Debug, clap::Parser)]
pub(crate) struct ToolSubcommand {
    #[clap(subcommand)]
    pub subcommand: ToolSubcommands,
}

#[derive(Debug, clap::Parser)]
pub(crate) enum ToolSubcommands {
    FormatAndDiff(FormatAndDiffSubcommand),
    Embed(Embed),
    WriteTccRspFile(WriteTccRspFile),
    BuildBinaryDep(build_binary_dep::BuildBinaryDepArgs),
}

pub(crate) fn run_tool(cli: &UniversalFlags, cmd: ToolSubcommand) -> anyhow::Result<i32> {
    match cmd.subcommand {
        ToolSubcommands::FormatAndDiff(subcmd) => run_format_and_diff(subcmd),
        ToolSubcommands::Embed(subcmd) => run_embed(subcmd),
        ToolSubcommands::WriteTccRspFile(subcmd) => write_tcc_rsp_file(subcmd),
        ToolSubcommands::BuildBinaryDep(subcmd) => {
            build_binary_dep::run_build_binary_dep(cli, &subcmd)
        }
    }
}
