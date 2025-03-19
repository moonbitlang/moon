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

use anyhow::{bail, Context, Result};
use colored::Colorize;
use dialoguer::Confirm;
use moonutil::common::{get_moon_version, get_moonc_version, get_moonrun_version, VersionItems};
use moonutil::moon_dir::{self};
use reqwest;
use reqwest::Client;
use std::time::Instant;
use tokio::time::timeout;

#[derive(Debug, clap::Parser, Clone)]
pub struct UpgradeSubcommand {
    /// Force upgrade
    #[clap(long, short)]
    pub force: bool,

    #[clap(long, hide = true)]
    pub non_interactive: bool,

    #[clap(long, hide = true)]
    pub base_url: Option<String>,
}

async fn check_latency(url: &str, client: &Client) -> Option<u128> {
    let start = Instant::now();
    let result = timeout(std::time::Duration::from_secs(1), client.get(url).send()).await;

    match result {
        Ok(Ok(resp)) if resp.status().is_success() => Some(start.elapsed().as_millis()),
        _ => None,
    }
}

async fn test_latency() -> Result<&'static str> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()
        .context("Failed to create HTTP client")?;

    let url1 = "https://cli.moonbitlang.com";
    let url2 = "https://cli.moonbitlang.cn";

    let url1_version = format!("{}/version.json", url1);
    let url2_version = format!("{}/version.json", url2);

    tokio::select! {
        res1 = check_latency(&url1_version, &client) => {
            if res1.is_some() {
                return Ok(url1);
            }
        }
        res2 = check_latency(&url2_version, &client) => {
            if res2.is_some() {
                return Ok(url2);
            }
        }
    }
    Ok(url1) // fall back to the first URL
}

fn check_connectivity() -> anyhow::Result<&'static str> {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async { test_latency().await })
}

fn extract_date(input: &str) -> Option<String> {
    // from the second dot
    input.split('.').nth(2).and_then(|s| {
        // find the first digit
        let start = s.find(|c: char| c.is_ascii_digit())?;
        // take 8 chars from the start
        let date = s[start..].chars().take(8).collect::<String>();
        // ensure the extracted string is 8 chars and all digits
        if date.len() == 8 && date.chars().all(|c| c.is_ascii_digit()) {
            Some(date)
        } else {
            None
        }
    })
}

#[test]
fn test_extract_date() {
    let date1 = extract_date("0.1.20240828 (901ac075 2024-08-28)").unwrap();
    assert_eq!("20240828", date1);
    let date2 = extract_date("v0.1.20240827+848d2bb76").unwrap();
    assert_eq!("20240827", date2);
    assert!(date1 > date2);
}

fn should_upgrade(latest_version_info: &VersionItems) -> Option<bool> {
    let moon_version = get_moon_version();
    let moonrun_version = get_moonrun_version().ok()?;
    let moonc_version = get_moonc_version().ok()?;

    // extract date from moon_version and moonc_version, compare with latest
    let moon_date = extract_date(&moon_version)?;
    let moonrun_date = extract_date(&moonrun_version)?;
    let moonc_date = extract_date(&moonc_version)?;
    let mut should_upgrade = false;
    for item in &latest_version_info.items {
        let latest_date = extract_date(&item.version)?;

        if ((item.name == "moon") && latest_date > moon_date)
            || (item.name == "moonrun" && latest_date > moonrun_date)
            || (item.name == "moonc" && latest_date > moonc_date)
        {
            should_upgrade = true;
        }
    }

    Some(should_upgrade)
}

pub fn upgrade(cmd: UpgradeSubcommand) -> Result<i32> {
    ctrlc::set_handler(upgrade_dialoguer_ctrlc_handler)?;
    let h = moon_dir::home();

    println!("Checking network ...");
    let root = if cmd.base_url.is_none() {
        check_connectivity()?.to_string()
    } else {
        cmd.base_url.unwrap().to_string()
    };
    println!("  Use {}", root);

    let download_page = if root.contains("moonbitlang.cn") {
        "https://www.moonbitlang.cn/download"
    } else {
        "https://www.moonbitlang.com/download"
    };

    println!("Checking latest toolchain version ...");
    let version_url = format!("{}/version.json", root);
    if !cmd.force {
        // if any step(network request, serde json...) fail, just do upgrade
        if let Ok(data) = reqwest::blocking::get(version_url) {
            if let Ok(latest_version_info) = data.json::<VersionItems>() {
                if let Some(false) = should_upgrade(&latest_version_info) {
                    println!("Your toolchain is up to date.");
                    return Ok(0);
                }
            }
        }
    }

    println!("{}", "Warning: moon upgrade is highly experimental.".bold());
    let msg = format!(
        "If you encounter any problems, please reinstall by visit {}",
        download_page
    );
    println!("{}", msg.bold());
    if !cmd.non_interactive {
        let confirm = Confirm::new()
            .with_prompt(format!(
                "Will install to {}. Continue?",
                h.display().to_string().bold()
            ))
            .default(true)
            .interact()?;
        if confirm {
            do_upgrade(&root)?;
        }
    } else {
        do_upgrade(&root)?;
    }
    println!("{}", "Done".green().bold());
    Ok(0)
}

pub fn do_upgrade(root: &str) -> Result<i32> {
    #[cfg(unix)]
    do_upgrade_unix(root)?;

    #[cfg(windows)]
    windows::do_upgrade_windows(root)?;

    Ok(0)
}

pub fn do_upgrade_unix(root: &str) -> Result<i32> {
    let exe = "sh";
    let args = [
        "-c".to_string(),
        format!("curl -fsSL {}/install/unix.sh | bash", root),
    ];
    let command = format!("{} {} '{}'", exe, args[0], args[1..].join(" "));
    let status = std::process::Command::new(exe)
        .args(args)
        .status()
        .with_context(|| format!("failed to execute command: {}", command))?;

    match status.code() {
        Some(0) => Ok(0),
        _ => bail!("failed to execute command: {}", command),
    }
}

pub fn upgrade_dialoguer_ctrlc_handler() {
    #[cfg(windows)]
    windows::copy_moon_back();

    moonutil::common::dialoguer_ctrlc_handler();
}

#[cfg(windows)]
mod windows {
    use super::*;
    pub static MOON_EXE_PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    pub static TEMP_EXE_PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

    pub fn copy_moon_back() {
        let _ = std::fs::copy(TEMP_EXE_PATH.get().unwrap(), MOON_EXE_PATH.get().unwrap());
    }

    pub fn do_upgrade_windows(root: &str) -> Result<i32> {
        let tmp_dir = tempfile::tempdir().context("failed to create temp dir")?;
        let current_exe = std::env::current_exe().context("failed to get current moon.exe path")?;
        let temp_exe = tmp_dir.path().join("moon.exe");
        std::fs::copy(&current_exe, &temp_exe)
            .context("failed to copy moon.exe to temp directory")?;
        let _ = MOON_EXE_PATH.set(current_exe);
        let _ = TEMP_EXE_PATH.set(temp_exe);

        self_replace::self_delete().context("failed to delete current moon.exe")?;
        let exe = "powershell";
        let args = [
        "-Command".to_string(),
        format!("Set-ExecutionPolicy RemoteSigned -Scope CurrentUser; irm {}/install/powershell.ps1 | iex", root),
    ];
        let command = format!("{} {} \"{}\"", exe, args[0], args[1..].join(" "));
        let status = std::process::Command::new(exe)
            .args(&args)
            .status()
            .with_context(|| format!("failed to execute command: {}", command))?;

        match status.code() {
            Some(0) => Ok(0),
            _ => {
                copy_moon_back();
                bail!("failed to execute command: {}", command)
            }
        }
    }
}
