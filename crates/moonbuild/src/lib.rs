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
pub mod bundle;
pub mod check;
pub mod doc_http;
pub mod dry_run;
pub mod entry;
pub mod expect;
pub mod fmt;
pub mod gen;
pub mod new;
pub mod pre_build;
pub mod runtest;
pub mod section_capture;
pub mod upgrade;
pub mod watch;

use sysinfo::{ProcessExt, System, SystemExt};

pub const MOON_PID_NAME: &str = ".moon.pid";

pub fn watcher_is_running(pid_path: &std::path::Path) -> anyhow::Result<bool> {
    if !pid_path.exists() {
        return Ok(false);
    }

    let pid = std::fs::read_to_string(pid_path)?;
    let pid = pid.parse::<usize>()?;
    let pid = sysinfo::Pid::from(pid);
    let mut sys = System::new();
    sys.refresh_processes();
    if let Some(p) = sys.process(pid) {
        if p.name() == "moon" {
            Ok(true)
        } else {
            Ok(false)
        }
    } else {
        Ok(false)
    }
}
