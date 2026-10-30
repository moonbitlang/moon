// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use std::{fs::File, io::BufReader};

use anyhow::Context;
use moonutil::{MOON_HOME, cli_support::UniversalFlags, registry::Credentials};

/// Show login status and username
#[derive(Debug, clap::Parser)]
pub(crate) struct WhoamiSubcommand {}

pub(crate) fn run_whoami(_cli: &UniversalFlags, _cmd: WhoamiSubcommand) -> anyhow::Result<i32> {
    let credentials_path = MOON_HOME.credentials_path();
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
        println!("Logged in, but username is unavailable. Please run `moon login` again.");
    }

    Ok(0)
}
