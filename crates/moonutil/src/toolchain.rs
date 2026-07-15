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

//! MoonBit toolchain layout and executable resolution.
//!
//! This module groups facts about the installed MoonBit toolchain: its root,
//! shipped `bin`/`lib`/`include` directories, shipped standard-library
//! artifacts, and resolved tool executable paths. Project-local build layout
//! should live outside this module.

pub use crate::binaries::{BINARIES, CachedBinaries};
pub use crate::moon_dir::{
    MOON_DIRS, MoonDirs, RESERVED_BIN_NAMES, abort_core_in, abort_mi_in, bin, core, core_bundle,
    core_bundle_in, core_core, core_core_in, core_package_mi_in, home, include, is_toolchain_root,
    lib, toolchain_root, user_bin, why3_datadir, why3_libdir,
};
