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

use std::{fs::File, io::BufReader};

use anyhow::Context;
use moonutil::{cli::UniversalFlags, moon_dir, mooncakes::Credentials};

/// Show login status and username
#[derive(Debug, clap::Parser)]
pub(crate) struct WhoamiSubcommand {}

pub(crate) fn run_whoami(_cli: &UniversalFlags, _cmd: WhoamiSubcommand) -> anyhow::Result<i32> {
    let credentials_path = moon_dir::credentials_json();
    if !credentials_path.exists() {
        println!("Not logged in");
        return Ok(0);
    }

    let file = File::open(&credentials_path).with_context(|| {
        format!(
            "failed to open credentials file `{}`",
            credentials_path.display()
        )
    })?;
    let credentials: Credentials = serde_json_lenient::from_reader(BufReader::new(file))
        .with_context(|| {
            format!(
                "failed to parse credentials file `{}`",
                credentials_path.display()
            )
        })?;

    if credentials.token.trim().is_empty() {
        println!("Not logged in");
    } else if let Some(username) = credentials.username.filter(|u| !u.trim().is_empty()) {
        println!("Logged in as {username}");
    } else {
        println!("Logged in (username unavailable)");
    }

    Ok(0)
}
