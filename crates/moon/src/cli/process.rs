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

use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

pub(crate) struct EarlySubcommand<'a> {
    pub(crate) current_dir: Option<PathBuf>,
    pub(crate) moon_trace: bool,
    pub(crate) name: &'a OsStr,
    pub(crate) args: &'a [OsString],
}

/// Locate a top-level subcommand before clap has successfully parsed the CLI.
///
/// Early delegation must interpret global options exactly far enough to apply
/// the same effective `-C` directory and explicit tracing policy as a normally
/// parsed subcommand. Once the subcommand is found, its arguments are opaque:
/// a later `--trace` belongs to the delegated command, not Moon.
pub(crate) fn early_subcommand(raw_args: &[OsString]) -> Option<EarlySubcommand<'_>> {
    let mut current_dir = None;
    let mut moon_trace = false;
    let mut index = 1;
    while index < raw_args.len() {
        let arg = &raw_args[index];
        match arg.to_str() {
            Some("-C") => {
                index += 1;
                current_dir = Some(PathBuf::from(raw_args.get(index)?));
            }
            Some(arg) if arg.starts_with("-C") && arg.len() > 2 => {
                let dir = &arg[2..];
                if dir.is_empty() || dir == "=" {
                    return None;
                }
                current_dir = Some(PathBuf::from(dir.strip_prefix('=').unwrap_or(dir)));
            }
            Some("--target-dir" | "--unstable-feature" | "-Z") => {
                index += 1;
                raw_args.get(index)?;
            }
            Some(arg)
                if arg.starts_with("--target-dir=")
                    || arg.starts_with("--unstable-feature=")
                    || (arg.starts_with("-Z") && arg.len() > 2) => {}
            Some("--trace") => moon_trace = true,
            Some(
                "-V" | "--version" | "-q" | "--quiet" | "-v" | "--verbose" | "--dry-run"
                | "--build-graph",
            ) => {}
            Some(arg) if arg.starts_with('-') && !arg.starts_with("--") && arg.len() > 2 => {
                let mut flags = &arg[1..];
                while matches!(flags.as_bytes().first().copied(), Some(b'V' | b'q' | b'v')) {
                    flags = &flags[1..];
                }
                if flags.is_empty() {
                    index += 1;
                    continue;
                }
                if let Some(dir) = flags.strip_prefix('C') {
                    let dir = dir.strip_prefix('=').unwrap_or(dir);
                    if dir.is_empty() {
                        index += 1;
                        current_dir = Some(PathBuf::from(raw_args.get(index)?));
                    } else {
                        current_dir = Some(PathBuf::from(dir));
                    }
                } else if flags == "Z" {
                    index += 1;
                    raw_args.get(index)?;
                } else if !flags.starts_with('Z') {
                    return Some(EarlySubcommand {
                        current_dir,
                        moon_trace,
                        name: &raw_args[index],
                        args: &raw_args[index + 1..],
                    });
                }
            }
            _ => {
                return Some(EarlySubcommand {
                    current_dir,
                    moon_trace,
                    name: arg,
                    args: &raw_args[index + 1..],
                });
            }
        }
        index += 1;
    }
    None
}

/// Construct a delegated command whose executable lookup and child working
/// directory use the same effective command directory.
pub(crate) fn command_in_effective_dir(
    current_dir: Option<&Path>,
    resolve_program: impl FnOnce(Option<&Path>) -> anyhow::Result<PathBuf>,
) -> anyhow::Result<Command> {
    let mut command = Command::new(resolve_program(current_dir)?);
    if let Some(dir) = current_dir {
        command.current_dir(dir);
    }
    Ok(command)
}

/// Delegate to a command as directly as the platform allows.
///
/// On Unix this replaces the current process. On Windows there is no direct
/// equivalent, so we run the child and wait while letting it handle Ctrl-C.
/// Use this only for command paths that return directly to process exit.
#[cfg(unix)]
pub(crate) fn delegate(cmd: &mut Command) -> anyhow::Result<ExitStatus> {
    use std::os::unix::prelude::*;

    Err(cmd.exec().into())
}

/// Keep the parent alive while its Windows console delivers Ctrl-C to children.
#[cfg(windows)]
pub(crate) fn install_ctrl_c_passthrough_handler() -> anyhow::Result<()> {
    use anyhow::bail;
    use windows_sys::Win32::Foundation::{BOOL, FALSE, TRUE};
    use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

    unsafe extern "system" fn ctrlc_handler(_: u32) -> BOOL {
        // The child receives the console event independently.
        TRUE
    }

    unsafe {
        if SetConsoleCtrlHandler(Some(ctrlc_handler), TRUE) == FALSE {
            bail!("could not set Ctrl-C handler")
        }
    }
    Ok(())
}

/// Delegate to a command as directly as the platform allows.
///
/// On Unix this replaces the current process. On Windows there is no direct
/// equivalent, so we run the child and wait while letting it handle Ctrl-C.
/// Use this only for command paths that return directly to process exit.
#[cfg(windows)]
pub(crate) fn delegate(cmd: &mut Command) -> anyhow::Result<ExitStatus> {
    install_ctrl_c_passthrough_handler()?;
    Ok(cmd.status()?)
}
