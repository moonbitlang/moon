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

//! Shared command-line types for MoonBit command frontends.
//!
//! Domain modules should not import through this module. It exists for command
//! adapters that need to share parsed flags and subcommand payloads.

pub use crate::cli::{UniversalFlags, dialoguer_ctrlc_handler};
pub use crate::mooncakes::{
    LoginSubcommand, MooncakeSubcommands, PackageSubcommand, PublishSubcommand, RegisterSubcommand,
    sync::AutoSyncFlags,
};
