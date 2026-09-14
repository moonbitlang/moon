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

use moonutil::cli_support::UniversalFlags;
use std::{env::current_exe, path::Path};

use anyhow::Context;
use moonutil::version::{get_moon_version, get_moonc_version, get_moonrun_version};

/// Print version information and exit
#[derive(Debug, clap::Parser)]
pub(crate) struct VersionSubcommand {
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

/// Single place to print the unstable feature footer for non-JSON output.
fn print_unstable_footer(flags: &UniversalFlags) {
    let features = flags.unstable_feature.to_string();
    if features.is_empty() {
        return;
    }

    println!();
    println!("Feature flags enabled: {features}");
}

pub(crate) fn run_version(flags: &UniversalFlags, cmd: VersionSubcommand) -> anyhow::Result<i32> {
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
            if nopath_flag {
                println!("moon {moon_version}");
                println!("moonc {}", moonc_version?);
                println!("moonc {}", moonrun_version?);
            } else {
                println!("moon {} {}", moon_version, get_moon_path()?);
                println!(
                    "moonc {} {}",
                    moonc_version?,
                    replace_home_with_tilde(&moonutil::toolchain::BINARIES.moonc)?
                );
                println!(
                    "{} {}",
                    moonrun_version?,
                    replace_home_with_tilde(&moonutil::toolchain::BINARIES.moonrun)?
                );
            }
            print_unstable_footer(flags);
        }
        (false, true) => {
            let items = moonutil::version::VersionItems {
                items: vec![moonutil::version::VersionItem {
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
            let items = moonutil::version::VersionItems {
                items: vec![
                    moonutil::version::VersionItem {
                        name: "moon".to_string(),
                        version: moon_version,
                        path: if nopath_flag {
                            None
                        } else {
                            Some(get_moon_path()?)
                        },
                    },
                    moonutil::version::VersionItem {
                        name: "moonc".to_string(),
                        version: moonc_version?,
                        path: if nopath_flag {
                            None
                        } else {
                            Some(replace_home_with_tilde(
                                &moonutil::toolchain::BINARIES.moonc,
                            )?)
                        },
                    },
                    moonutil::version::VersionItem {
                        name: "moonrun".to_string(),
                        version: moonrun_version?,
                        path: if nopath_flag {
                            None
                        } else {
                            Some(replace_home_with_tilde(
                                &moonutil::toolchain::BINARIES.moonrun,
                            )?)
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
