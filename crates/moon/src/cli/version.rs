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

use moonutil::cli::UniversalFlags;
use std::{env::current_exe, path::Path};

use anyhow::Context;
use moonutil::common::{
    get_moon_version, get_moonc_version, get_moonrun_version, get_program_version,
};
use which::which_global;

/// Print version information and exit
#[derive(Debug, clap::Parser)]
pub struct VersionSubcommand {
    /// Print all version information
    #[clap(long)]
    pub all: bool,

    /// Print version information in JSON format
    #[clap(long)]
    pub json: bool,

    /// Do not print the path
    #[clap(long)]
    pub no_path: bool,
}

fn replace_home_with_tilde(p: &Path) -> anyhow::Result<String> {
    let h = home::home_dir().context("failed to get home directory")?;
    Ok(if p.starts_with(&h) {
        p.display()
            .to_string()
            .replacen(&h.display().to_string(), "~", 1)
    } else {
        p.display().to_string()
    })
}

fn get_moon_path() -> anyhow::Result<String> {
    let moon_path = current_exe().context("failed to get current executable path")?;
    replace_home_with_tilde(&moon_path)
}

/// Single place to print the unstable footer (features + notice) for non-JSON output.
fn print_unstable_footer(flags: &UniversalFlags) {
    let features = flags.unstable_feature.to_string();
    if features.is_empty() {
        return;
    }

    println!();
    println!("Feature flags enabled: {features}");
    if flags.unstable_feature.rupes_recta {
        println!(
            "-> You're currently using the experimental build graph generator \"Rupes Recta\". \
            If you encounter a problem, \
            please verify whether it also reproduces with the legacy build (by setting NEW_MOON=0)."
        )
    }
}

pub fn run_version(flags: &UniversalFlags, cmd: VersionSubcommand) -> anyhow::Result<i32> {
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
            println!("moon {moon_version}");
            print_unstable_footer(flags);
        }
        (true, false) => {
            let moon_pilot_path = which_global("moon-pilot");
            if nopath_flag {
                println!("moon {moon_version}");
                println!("moonc {}", moonc_version?);
                println!("moonc {}", moonrun_version?);
                if let Ok(moon_pilot_path) = moon_pilot_path {
                    println!("moon-pilot {}", get_program_version(&moon_pilot_path)?);
                }
            } else {
                println!("moon {} {}", moon_version, get_moon_path()?);
                println!(
                    "moonc {} {}",
                    moonc_version?,
                    replace_home_with_tilde(&moonutil::BINARIES.moonc)?
                );
                println!(
                    "{} {}",
                    moonrun_version?,
                    replace_home_with_tilde(&moonutil::BINARIES.moonrun)?
                );
                if let Ok(moon_pilot_path) = moon_pilot_path {
                    println!(
                        "moon-pilot {} {}",
                        get_program_version(&moon_pilot_path)?,
                        replace_home_with_tilde(&moon_pilot_path)?
                    );
                }
            }
            print_unstable_footer(flags);
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
                serde_json_lenient::to_string(&items)
                    .context("failed to serialize version info to JSON")?
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
                            Some(replace_home_with_tilde(&moonutil::BINARIES.moonc)?)
                        },
                    },
                    moonutil::common::VersionItem {
                        name: "moonrun".to_string(),
                        version: moonrun_version?,
                        path: if nopath_flag {
                            None
                        } else {
                            Some(replace_home_with_tilde(&moonutil::BINARIES.moonrun)?)
                        },
                    },
                ],
            };
            println!(
                "{}",
                serde_json_lenient::to_string(&items)
                    .context("failed to serialize version info to JSON")?
            );
        }
    }
    Ok(0)
}
