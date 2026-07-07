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

pub use crate::build_options::*;
pub use crate::cli::dialoguer_ctrlc_handler;
pub use crate::constants::*;
pub use crate::front_matter::*;
pub use crate::glob::*;
pub use crate::locks::FileLock;
pub use crate::manifest::*;
pub use crate::path::{CargoPathExt, get_desc_name};
pub use crate::render::{PatchItem, PatchJSON};
pub use crate::scripts::*;
pub use crate::target::*;
pub use crate::test_metadata::*;
pub use crate::text::*;
pub use crate::version::{
    VersionItem, VersionItems, get_cargo_pkg_version, get_moon_version, get_moonc_version,
    get_moonrun_version, get_program_version,
};
