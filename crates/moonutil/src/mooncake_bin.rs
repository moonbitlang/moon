// Copyright 2024 International Digital Economy Academy
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
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

//! Interacts with the `mooncake` binary

use std::path::PathBuf;

fn determine_moon_bin() -> Option<PathBuf> {
    // Check if the `mooncake` binary is in the executable's directory
    let curr_exe = std::env::current_exe();
    if let Ok(curr_exe) = curr_exe {
        let mut moon_bin = curr_exe.clone();
        moon_bin.set_file_name("moon");
        #[cfg(windows)]
        {
            moon_bin.set_extension("exe");
        }
        if moon_bin.is_file() {
            return Some(moon_bin);
        }
    }
    None
}

pub fn call_moon_from_mooncake() -> std::process::Command {
    std::process::Command::new(determine_moon_bin().unwrap_or_else(|| "moon".into()))
}

fn determine_mooncake_bin() -> Option<PathBuf> {
    // Check if the `mooncake` binary is in the executable's directory
    let curr_exe = std::env::current_exe();
    if let Ok(curr_exe) = curr_exe {
        let mut mooncake_bin = curr_exe.clone();
        mooncake_bin.set_file_name("mooncake");
        #[cfg(windows)]
        {
            mooncake_bin.set_extension("exe");
        }
        if mooncake_bin.is_file() {
            return Some(mooncake_bin);
        }
    }
    None
}

pub fn call_mooncake() -> std::process::Command {
    std::process::Command::new(determine_mooncake_bin().unwrap_or_else(|| "mooncake".into()))
}
