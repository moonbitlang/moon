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

use anyhow::{Context, Result, bail};
use colored::Colorize;
use dialoguer::Confirm;
use moonutil::MOON_HOME;
use moonutil::toolchain;
use moonutil::version::{VersionItems, get_moon_version, get_moonc_version, get_moonrun_version};
use reqwest::Client;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::time::timeout;

use super::UpgradeSubcommand;

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

    let url1_version = format!("{url1}/version.json");
    let url2_version = format!("{url2}/version.json");

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

fn upgrade_home() -> Result<PathBuf> {
    let home = MOON_HOME.root();
    let toolchain_root = toolchain::toolchain_root();
    let canonicalize =
        |path: &Path| dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

    if canonicalize(home) != canonicalize(&toolchain_root) {
        bail!(
            "moon upgrade only supports toolchains installed under MOON_HOME. Active toolchain root is `{}`, but MOON_HOME is `{}`. Please upgrade this installation with the package manager or installer that owns the toolchain.",
            toolchain_root.display(),
            home.display()
        );
    }

    Ok(home.to_path_buf())
}

/// Directories that the installers delete before they have a verified
/// replacement (see `install/powershell.ps1` and `install/unix.sh`). `bin` is
/// left out: it holds the executable that is currently running, which cannot be
/// moved aside while it is in use, and it already has its own single-file
/// recovery path in `copy_moon_back`. A failed upgrade can therefore still
/// leave `bin` partially replaced; re-running the upgrade repairs it.
const REPLACED_DIRS: [&str; 2] = ["lib", "include"];
const BACKUP_SUFFIX: &str = ".bak-upgrade";

/// The toolchain directories that an upgrade is about to overwrite, kept until
/// the replacement has been verified. Renaming inside `MOON_HOME` is a metadata
/// operation, so this is cheap even though `lib` is a few hundred megabytes.
static BACKUP: std::sync::OnceLock<ToolchainBackup> = std::sync::OnceLock::new();

struct ToolchainBackup {
    home: PathBuf,
    moved: Vec<(PathBuf, PathBuf)>,
}

impl ToolchainBackup {
    /// Take the directories aside, so the installer has somewhere to write and
    /// the previous tree can come back.
    ///
    /// A backup still present here means an earlier run could not restore it,
    /// which makes it the only copy of that tree. Refuse to start instead of
    /// clearing it: the earlier failure is then reported, not turned into data
    /// loss.
    ///
    /// The same bar applies to a directory that cannot be moved aside: the
    /// installer deletes what it finds, so a tree that cannot be kept has to
    /// stop the upgrade before the installer runs.
    fn capture(home: &Path) -> Result<Self> {
        for name in REPLACED_DIRS {
            let backup = home.join(format!("{name}{BACKUP_SUFFIX}"));
            if backup.exists() {
                bail!(
                    "cannot start the upgrade: `{}` is still present from an earlier run that \
                     could not restore it. It holds the previous `{name}`; inspect or remove it, \
                     then run `moon upgrade` again.",
                    backup.display()
                );
            }
        }
        let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
        for name in REPLACED_DIRS {
            let original = home.join(name);
            if !original.is_dir() {
                continue;
            }
            let backup = home.join(format!("{name}{BACKUP_SUFFIX}"));
            if let Err(err) = std::fs::rename(&original, &backup) {
                // Put back what was already moved, so the upgrade can be
                // retried without a half-taken-apart tree.
                for (original, backup) in moved.iter().rev() {
                    let _ = std::fs::rename(backup, original);
                }
                bail!(
                    "cannot move `{}` aside before the upgrade: {err}. The installer deletes it, \
                     so the upgrade stops before anything is lost.",
                    original.display()
                );
            }
            moved.push((original, backup));
        }
        Ok(Self {
            home: home.to_path_buf(),
            moved,
        })
    }

    fn restore(&self) {
        let mut restored = Vec::new();
        for (original, backup) in &self.moved {
            if !backup.exists() {
                continue;
            }
            match swap_in(backup, original) {
                Ok(()) => restored.push(original.display().to_string()),
                Err(err) => eprintln!(
                    "warning: the previous tree is kept at `{}`: {err}",
                    backup.display()
                ),
            }
        }
        if restored.is_empty() {
            return;
        }
        // What was put back is whatever the installer was given, so say what is
        // actually there: claiming a restore over a half tree is how a broken
        // installation gets mistaken for a working one.
        let missing = incomplete_parts(&self.home);
        if missing.is_empty() {
            eprintln!(
                "Restored the previous toolchain ({}) after the failed upgrade.",
                restored.join(", ")
            );
        } else {
            eprintln!(
                "warning: restored the previous toolchain ({}), but it is still incomplete \
                 (missing {}); reinstall from https://www.moonbitlang.com/download",
                restored.join(", "),
                missing.join(", ")
            );
        }
    }

    fn commit(&self) {
        for (_, backup) in &self.moved {
            if let Err(err) = std::fs::remove_dir_all(backup) {
                eprintln!("warning: cannot remove `{}`: {err}", backup.display());
            }
        }
    }
}

/// Move `backup` back to `original`. Windows refuses to rename onto an existing
/// directory, so anything the installer left behind is moved aside first and
/// only dropped once the restore has succeeded.
fn swap_in(backup: &Path, original: &Path) -> std::io::Result<()> {
    let aside = original.with_extension("failed-upgrade");
    let mut moved_aside = false;
    if original.exists() {
        let _ = std::fs::remove_dir_all(&aside);
        std::fs::rename(original, &aside)?;
        moved_aside = true;
    }
    match std::fs::rename(backup, original) {
        Ok(()) => {
            if moved_aside {
                let _ = std::fs::remove_dir_all(&aside);
            }
            Ok(())
        }
        Err(err) => {
            if moved_aside {
                let _ = std::fs::rename(&aside, original);
            }
            Err(err)
        }
    }
}

fn has_entries(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_some())
}

fn has_core_manifest(core: &Path) -> bool {
    core.join("moon.mod").is_file() || core.join("moon.mod.json").is_file()
}

/// Whether `lib` holds a core `moon` can build against. A half-extracted core
/// leaves the directory behind but no module manifest, which is what `moon`
/// loads when it injects the standard library.
fn lib_replaced(home: &Path) -> bool {
    let core = home.join("lib").join("core");
    has_entries(&core) && has_core_manifest(&core)
}

/// Whether the installer got as far as replacing `name`. This is the same bar
/// `incomplete_parts` applies, so recovery and verification cannot disagree on
/// what counts as a replacement.
fn part_replaced(home: &Path, name: &str) -> bool {
    match name {
        "lib" => lib_replaced(home),
        _ => has_entries(&home.join(name)),
    }
}

fn incomplete_parts(home: &Path) -> Vec<String> {
    let mut missing = Vec::new();
    let core = home.join("lib").join("core");
    if !has_entries(&core) {
        missing.push("lib/core".to_string());
    } else if !has_core_manifest(&core) {
        missing.push("lib/core/moon.mod".to_string());
    }
    if !has_entries(&home.join("include")) {
        missing.push("include".to_string());
    }
    if !home
        .join("bin")
        .join(format!("moonc{}", std::env::consts::EXE_SUFFIX))
        .is_file()
    {
        missing.push("bin/moonc".to_string());
    }
    missing
}

fn verify_installed(home: &Path) -> Result<()> {
    let incomplete = incomplete_parts(home);
    if incomplete.is_empty() {
        return Ok(());
    }
    bail!(
        "the upgrade finished with an incomplete toolchain at `{}`: missing {}",
        home.display(),
        incomplete.join(", ")
    )
}

fn restore_toolchain_backup() {
    if let Some(backup) = BACKUP.get() {
        backup.restore();
    }
}

fn commit_toolchain_backup() {
    if let Some(backup) = BACKUP.get() {
        backup.commit();
    }
}

/// An upgrade can die between moving the previous tree aside and putting a
/// replacement in place (power loss, a closed console, a killed process). The
/// backup is then the only remaining copy, so put it back before anything else.
///
/// The installers replace `lib` and `include` together, so recovery judges them
/// together: a complete `lib` beside a missing `include` is two versions in one
/// tree, and the whole backup set goes back rather than per directory. What
/// counts as complete is `incomplete_parts`' bar, not "the directory has any
/// file" — a half-extracted core has files but no manifest, and taking that for
/// a replacement would delete the only good copy.
fn recover_interrupted_upgrade(home: &Path) -> Result<()> {
    let backups: Vec<(&str, PathBuf)> = REPLACED_DIRS
        .iter()
        .map(|name| (*name, home.join(format!("{name}{BACKUP_SUFFIX}"))))
        .filter(|(_, backup)| backup.exists())
        .collect();
    if backups.is_empty() {
        return Ok(());
    }
    if REPLACED_DIRS.iter().all(|name| part_replaced(home, name)) {
        // The replacement made it in; the backups are just leftovers.
        for (name, backup) in backups {
            if let Err(err) = std::fs::remove_dir_all(&backup) {
                eprintln!(
                    "warning: cannot remove the leftover backup of `{name}` at `{}`: {err}",
                    backup.display()
                );
            }
        }
        return Ok(());
    }
    for (name, backup) in backups {
        swap_in(&backup, &home.join(name)).map_err(|err| {
            anyhow::anyhow!(
                "cannot restore `{name}` from `{}`: {err}. It is still there; a later step could \
                 overwrite it, so the upgrade stops here.",
                backup.display()
            )
        })?;
        eprintln!("Restored `{name}` from an interrupted upgrade.");
    }
    Ok(())
}

pub(super) fn upgrade(cmd: UpgradeSubcommand) -> Result<i32> {
    let h = upgrade_home()?;
    recover_interrupted_upgrade(&h)?;
    ctrlc::set_handler(upgrade_dialoguer_ctrlc_handler)?;

    println!("Checking network ...");
    let root = if cmd.base_url.is_none() {
        check_connectivity()?.to_string()
    } else {
        cmd.base_url.as_deref().unwrap().to_string()
    };
    println!("  Use {root}");

    let download_page = if root.contains("moonbitlang.cn") {
        "https://www.moonbitlang.cn/download"
    } else {
        "https://www.moonbitlang.com/download"
    };

    println!("Checking latest toolchain version ...");
    let version_url = format!("{root}/version.json");
    if !cmd.force && !cmd.dev {
        // if any step(network request, serde json...) fail, just do upgrade
        if let Ok(data) = reqwest::blocking::get(version_url)
            && let Ok(latest_version_info) = data.json::<VersionItems>()
            && let Some(false) = should_upgrade(&latest_version_info)
        {
            println!(
                "Your toolchain is up to date. You can use `moon upgrade --force` to force upgrade."
            );
            return Ok(0);
        }
    }

    println!("{}", "Warning: moon upgrade is highly experimental.".bold());
    let msg = format!("If you encounter any problems, please reinstall by visit {download_page}");
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
            do_upgrade(&cmd, &root)?;
        }
    } else {
        do_upgrade(&cmd, &root)?;
    }
    println!("{}", "Done".green().bold());
    Ok(0)
}

fn do_upgrade(cmd: &UpgradeSubcommand, root: &str) -> Result<i32> {
    let home = MOON_HOME.root().to_path_buf();
    let _ = BACKUP.set(ToolchainBackup::capture(&home)?);

    match run_installer(cmd, root).and_then(|rc| verify_installed(&home).map(|()| rc)) {
        Ok(rc) => {
            commit_toolchain_backup();
            Ok(rc)
        }
        Err(err) => {
            restore_toolchain_backup();
            Err(err)
        }
    }
}

fn run_installer(cmd: &UpgradeSubcommand, root: &str) -> Result<i32> {
    #[cfg(unix)]
    do_upgrade_unix(cmd, root)?;

    #[cfg(windows)]
    windows::do_upgrade_windows(cmd, root)?;

    Ok(0)
}

#[cfg(unix)]
fn do_upgrade_unix(cmd: &UpgradeSubcommand, root: &str) -> Result<i32> {
    let exe = "sh";
    let args = [
        "-c".to_string(),
        format!(
            "curl -fsSL {}/install/unix.sh | {}bash",
            root,
            if cmd.dev {
                "MOONBIT_INSTALL_DEV=1 "
            } else {
                ""
            },
        ),
    ];
    let hint = format!("{} {} '{}'", exe, args[0], args[1..].join(" "));
    let status = std::process::Command::new(exe)
        .args(args)
        .status()
        .with_context(|| format!("failed to execute command: {hint}"))?;

    match status.code() {
        Some(0) => Ok(0),
        _ => bail!("failed to execute command: {}", hint),
    }
}

fn upgrade_dialoguer_ctrlc_handler() {
    #[cfg(windows)]
    windows::copy_moon_back();

    restore_toolchain_backup();

    let _ = dialoguer::console::Term::stdout().show_cursor();
    std::process::exit(1);
}

#[cfg(windows)]
mod windows {
    use super::*;
    static MOON_EXE_PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    static TEMP_EXE_PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

    pub(super) fn copy_moon_back() {
        if TEMP_EXE_PATH.get().is_some() && MOON_EXE_PATH.get().is_some() {
            let _ = std::fs::copy(TEMP_EXE_PATH.get().unwrap(), MOON_EXE_PATH.get().unwrap());
        }
    }

    pub(super) fn do_upgrade_windows(cmd: &UpgradeSubcommand, root: &str) -> Result<i32> {
        let tmp_dir = tempfile::tempdir().context("failed to create temp dir")?;
        match _do_upgrade_windows(cmd, root, tmp_dir.path()) {
            Ok(0) => Ok(0),
            e => {
                copy_moon_back();
                e
            }
        }
    }

    fn _do_upgrade_windows(
        cmd: &UpgradeSubcommand,
        root: &str,
        tmp_dir: &std::path::Path,
    ) -> Result<i32> {
        let current_exe = std::env::current_exe().context("failed to get current moon.exe path")?;
        let temp_exe = tmp_dir.join("moon.exe");
        std::fs::copy(&current_exe, &temp_exe)
            .context("failed to copy moon.exe to temp directory")?;
        let _ = MOON_EXE_PATH.set(current_exe.clone());
        let _ = TEMP_EXE_PATH.set(temp_exe);

        if current_exe.starts_with(MOON_HOME.root()) {
            self_replace::self_delete().context("failed to delete current moon.exe")?;
        }

        let mut script = String::new();
        if cmd.dev {
            script.push_str("$env:MOONBIT_INSTALL_DEV=1\n");
        }
        script.push_str("Set-ExecutionPolicy RemoteSigned -Scope CurrentUser\n");
        script.push_str(&format!("irm {}/install/powershell.ps1 | iex\n", root));
        if cmd.dev {
            script.push_str("Remove-Item Env:\\MOONBIT_INSTALL_DEV")
        }
        let script_path = tmp_dir.join("script.ps1");
        std::fs::write(&script_path, &script).context("failed to write install script")?;
        let exe = "powershell";
        let status = std::process::Command::new(exe)
            .arg(&script_path)
            // Let Windows PowerShell rebuild its module paths; a parent PowerShell 7
            // session can otherwise make it load incompatible modules through Moon.
            .env_remove("PSModulePath")
            .status()
            .with_context(|| format!("failed to execute command:\n{}", script))?;
        match status.code() {
            Some(0) => Ok(0),
            _ => {
                bail!("failed to execute command:\n{}", script)
            }
        }
    }
}
