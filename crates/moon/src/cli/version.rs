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

use std::{env::current_exe, path::Path};

use anyhow::Context;
use moonutil::common::{get_moon_version, get_moonc_version, get_moonrun_version};

/// Print version info and exit
#[derive(Debug, clap::Parser)]
pub struct VersionSubcommand {
    /// Print all version info
    #[clap(long)]
    pub all: bool,

    /// Print version info in JSON format
    #[clap(long)]
    pub json: bool,

    /// Do not print the path
    #[clap(long)]
    pub no_path: bool,
}

fn replace_home_with_tilde(p: &Path, h: &Path) -> String {
    if p.starts_with(h) {
        p.display()
            .to_string()
            .replacen(&h.display().to_string(), "~", 1)
    } else {
        p.display().to_string()
    }
}

fn get_moon_path() -> anyhow::Result<String> {
    let user_home = home::home_dir().context("failed to get home directory")?;
    let moon_path = which::which("moon");
    let moon_path = if let Ok(moon_path) = moon_path {
        moon_path
    } else {
        current_exe().context("failed to get current executable path")?
    };
    Ok(replace_home_with_tilde(&moon_path, &user_home))
}

fn get_moonc_path() -> anyhow::Result<String> {
    let moonc_path = which::which("moonc").context("failed to find moonc")?;
    let user_home = home::home_dir().context("failed to get home directory")?;
    Ok(replace_home_with_tilde(&moonc_path, &user_home))
}

fn get_moonrun_path() -> anyhow::Result<String> {
    let moonrun_path = which::which("moonrun").context("failed to find moonc")?;
    let user_home = home::home_dir().context("failed to get home directory")?;
    Ok(replace_home_with_tilde(&moonrun_path, &user_home))
}

pub fn run_version(cmd: VersionSubcommand) -> anyhow::Result<i32> {
    let VersionSubcommand {
        all: all_flag,
        json: json_flag,
        no_path: nopath_flag,
    } = cmd;

    let (moon_version, moonc_version, moonrun_version) = (
        get_moon_version(),
        get_moonc_version(),
        get_moonrun_version(),
    );

    match (all_flag, json_flag) {
        (false, false) => {
            println!("moon {}", moon_version);
        }
        (true, false) => {
            if nopath_flag {
                println!("moon {}", moon_version);
                println!("moonc {}", moonc_version?);
                println!("moonc {}", moonrun_version?);
            } else {
                println!("moon {} {}", moon_version, get_moon_path()?);
                println!("moonc {} {}", moonc_version?, get_moonc_path()?);
                println!("{} {}", moonrun_version?, get_moonrun_path()?);
            }
        }
        (false, true) => {
            let items = moonutil::common::VersionItems {
                items: vec![moonutil::common::VersionItem {
                    name: "moon".to_string(),
                    version: moon_version,
                    path: if nopath_flag {
                        None
                    } else {
                        Some(get_moon_path()?)
                    },
                }],
            };
            println!(
                "{}",
                serde_json_lenient::to_string(&items).context(format!(
                    "failed to serialize version info to JSON: {:#?}",
                    items
                ))?
            );
        }
        (true, true) => {
            let items = moonutil::common::VersionItems {
                items: vec![
                    moonutil::common::VersionItem {
                        name: "moon".to_string(),
                        version: moon_version,
                        path: if nopath_flag {
                            None
                        } else {
                            Some(get_moon_path()?)
                        },
                    },
                    moonutil::common::VersionItem {
                        name: "moonc".to_string(),
                        version: moonc_version?,
                        path: if nopath_flag {
                            None
                        } else {
                            Some(get_moonc_path()?)
                        },
                    },
                    moonutil::common::VersionItem {
                        name: "moonrun".to_string(),
                        version: moonrun_version?,
                        path: if nopath_flag {
                            None
                        } else {
                            Some(get_moonrun_path()?)
                        },
                    },
                ],
            };
            println!(
                "{}",
                serde_json_lenient::to_string(&items).context(format!(
                    "failed to serialize version info to JSON: {:#?}",
                    items
                ))?
            );
        }
    }
    Ok(0)
}
