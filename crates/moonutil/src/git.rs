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

use std::path::Path;

use colored::Colorize;

pub fn is_in_git_repo(path: &Path) -> bool {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .current_dir(path)
        .status();
    match output {
        Ok(out) => out.success(),
        _ => false,
    }
}

pub fn create_git_repo(path: &Path) {
    let git_init = std::process::Command::new("git")
        .arg("init")
        .current_dir(path)
        .status();
    match git_init {
        Ok(o) => if o.success() {},
        _ => {
            eprintln!(
                "{}: git init failed, make sure you have git in PATH",
                "Warning".yellow().bold()
            );
        }
    }
}
