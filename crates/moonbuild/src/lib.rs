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

#![warn(clippy::clone_on_ref_ptr)]

pub mod bench;
pub mod benchmark;
pub mod build;
pub mod build_script;
pub mod bundle;
pub mod check;
pub mod doc_http;
pub mod dry_run;
pub mod entry;
pub mod expect;
pub mod fmt;
pub mod r#gen;
pub mod new;
pub mod pre_build;
pub mod runtest;
pub mod section_capture;
pub mod test_utils;
pub mod upgrade;

use std::sync::LazyLock;

static NODE_EXECUTABLE: LazyLock<Option<std::path::PathBuf>> = LazyLock::new(|| {
    ["node.cmd", "node"]
        .iter()
        .find_map(|name| which::which(name).ok())
});
static PYTHON_EXECUTABLE: LazyLock<Option<std::path::PathBuf>> = LazyLock::new(|| {
    ["python3", "python", "python3.exe", "python.exe"]
        .iter()
        .find_map(|name| which::which(name).ok())
});
