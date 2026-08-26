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

//! Side-effect-free selection of one top-level invocation.

use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use clap::{CommandFactory, Parser, error::ErrorKind};
use moonutil::cli_support::UniversalFlags;

use super::{CramCommand, MoonBuildCli, MoonBuildSubcommands, VersionSubcommand, moonx, tool};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputFormat {
    Human,
    Json,
}

#[derive(Debug)]
pub(crate) struct MoonInvocation {
    pub(crate) flags: UniversalFlags,
    pub(crate) command: MoonBuildSubcommands,
    pub(crate) output: OutputFormat,
}

#[derive(Debug)]
pub(crate) enum DelegatedInvocation {
    ToolExec(tool::exec::Exec),
    External {
        current_dir: Option<PathBuf>,
        args: Vec<OsString>,
    },
    IdeHelp {
        current_dir: Option<PathBuf>,
        args: Vec<OsString>,
    },
    Cram {
        current_dir: Option<PathBuf>,
        args: Vec<OsString>,
    },
    Login {
        current_dir: Option<PathBuf>,
    },
    Register {
        current_dir: Option<PathBuf>,
    },
}

#[derive(Debug)]
pub(crate) enum SelectedInvocation {
    Help,
    Moon(Box<MoonInvocation>),
    Moonx(moonx::MoonxInvocation),
    Delegate(DelegatedInvocation),
}

/// Parse and classify an invocation before output, tracing, workspace state,
/// current-directory changes, or child processes are initialized.
pub(crate) fn select(raw_args: Vec<OsString>) -> Result<SelectedInvocation, clap::Error> {
    if moonx::is_moonx_invocation(&raw_args) {
        return moonx::parse_from(&raw_args).map(SelectedInvocation::Moonx);
    }
    if tool::exec::is_tool_exec(&raw_args) {
        return tool::exec::parse_from_raw_args(&raw_args)
            .map(DelegatedInvocation::ToolExec)
            .map(SelectedInvocation::Delegate);
    }
    let external_cram = select_external_cram(&raw_args);
    let parsed = match MoonBuildCli::try_parse_from(&raw_args) {
        Ok(parsed) => parsed,
        Err(error) => {
            let delegated = match error.kind() {
                ErrorKind::InvalidSubcommand => select_ide_help(&raw_args),
                ErrorKind::UnknownArgument => external_cram,
                _ => None,
            };
            return delegated.map_or(Err(error), |(name, explicit_trace, invocation)| {
                select_transparent_delegate(name, explicit_trace, invocation)
            });
        }
    };

    let MoonBuildCli {
        subcommand,
        version,
        flags,
    } = parsed;
    // Cram owns an opaque argument tail. Clap accepts global flags after a
    // subcommand, so a successful parse can still have consumed an argument
    // that belongs to moon-cram. Reconcile that ownership here, except when
    // Moon's successfully parsed global version flag selects Moon itself.
    if !version && let Some((name, explicit_trace, invocation)) = external_cram {
        return select_transparent_delegate(name, explicit_trace, invocation);
    }
    let command = if version {
        MoonBuildSubcommands::Version(VersionSubcommand {
            all: true,
            json: false,
            no_path: false,
        })
    } else if let Some(command) = subcommand {
        command
    } else {
        return Ok(SelectedInvocation::Help);
    };

    let current_dir = flags.source_tgt_dir.cwd.clone();
    match command {
        MoonBuildSubcommands::External(args) => {
            let name = args
                .first()
                .cloned()
                .unwrap_or_else(|| "external".to_owned());
            select_transparent_delegate(
                &name,
                flags.trace,
                DelegatedInvocation::External {
                    current_dir,
                    args: args.into_iter().map(OsString::from).collect(),
                },
            )
        }
        MoonBuildSubcommands::Cram(super::CramSubcommand {
            command: Some(CramCommand::External(args)),
        }) => select_transparent_delegate(
            "cram",
            flags.trace,
            DelegatedInvocation::Cram {
                current_dir,
                args: args.into_iter().map(OsString::from).collect(),
            },
        ),
        MoonBuildSubcommands::Cram(super::CramSubcommand { command: None }) => {
            select_transparent_delegate(
                "cram",
                flags.trace,
                DelegatedInvocation::Cram {
                    current_dir,
                    args: Vec::new(),
                },
            )
        }
        MoonBuildSubcommands::Login(_) => select_transparent_delegate(
            "login",
            flags.trace,
            DelegatedInvocation::Login { current_dir },
        ),
        MoonBuildSubcommands::Register(_) => select_transparent_delegate(
            "register",
            flags.trace,
            DelegatedInvocation::Register { current_dir },
        ),
        command => {
            let output = match &command {
                MoonBuildSubcommands::Check(command) if command.json => OutputFormat::Json,
                MoonBuildSubcommands::Search(command) if command.json => OutputFormat::Json,
                _ => OutputFormat::Human,
            };
            Ok(SelectedInvocation::Moon(Box::new(MoonInvocation {
                flags,
                command,
                output,
            })))
        }
    }
}

fn select_transparent_delegate(
    command: &str,
    explicit_trace: bool,
    invocation: DelegatedInvocation,
) -> Result<SelectedInvocation, clap::Error> {
    if explicit_trace {
        Err(trace_conflict(command))
    } else {
        Ok(SelectedInvocation::Delegate(invocation))
    }
}

fn trace_conflict(command: &str) -> clap::Error {
    MoonBuildCli::command().error(
        ErrorKind::ArgumentConflict,
        format!("`--trace` is not supported before delegated command `{command}`"),
    )
}

struct EarlySubcommand<'a> {
    current_dir: Option<PathBuf>,
    explicit_trace: bool,
    name: &'a OsStr,
    args: &'a [OsString],
}

/// Parse only Moon-owned global options before an opaque delegation point.
fn early_subcommand(raw_args: &[OsString]) -> Option<EarlySubcommand<'_>> {
    let mut current_dir = None;
    let mut explicit_trace = false;
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
            Some("--trace") => explicit_trace = true,
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
                        explicit_trace,
                        name: &raw_args[index],
                        args: &raw_args[index + 1..],
                    });
                }
            }
            _ => {
                return Some(EarlySubcommand {
                    current_dir,
                    explicit_trace,
                    name: arg,
                    args: &raw_args[index + 1..],
                });
            }
        }
        index += 1;
    }
    None
}

fn select_ide_help(raw_args: &[OsString]) -> Option<(&'static str, bool, DelegatedInvocation)> {
    let early = early_subcommand(raw_args)?;
    if early.name != OsStr::new("help") {
        return None;
    }
    let [ide, tail @ ..] = early.args else {
        return None;
    };
    if ide != OsStr::new("ide") {
        return None;
    }

    let mut args = tail.to_vec();
    args.push(OsString::from("--help"));
    Some((
        "ide",
        early.explicit_trace,
        DelegatedInvocation::IdeHelp {
            current_dir: early.current_dir,
            args,
        },
    ))
}

fn select_external_cram(
    raw_args: &[OsString],
) -> Option<(&'static str, bool, DelegatedInvocation)> {
    let early = early_subcommand(raw_args)?;
    (early.name == OsStr::new("cram")
        && matches!(
            early.args.first(),
            Some(arg)
                if arg != OsStr::new("test")
                    && arg != OsStr::new("--help")
                    && arg != OsStr::new("-h")
        ))
    .then(|| {
        (
            "cram",
            early.explicit_trace,
            DelegatedInvocation::Cram {
                current_dir: early.current_dir,
                args: early.args.to_vec(),
            },
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn selects_human_and_json_moon_commands() {
        let SelectedInvocation::Moon(build) = select(args(&["moon", "build"])).unwrap() else {
            panic!("build should select a Moon invocation")
        };
        assert_eq!(build.output, OutputFormat::Human);

        let SelectedInvocation::Moon(check) = select(args(&["moon", "check", "--json"])).unwrap()
        else {
            panic!("JSON check should select a Moon invocation")
        };
        assert_eq!(check.output, OutputFormat::Json);

        let SelectedInvocation::Moon(search) =
            select(args(&["moon", "search", "json", "--json"])).unwrap()
        else {
            panic!("JSON search should select a Moon invocation")
        };
        assert_eq!(search.output, OutputFormat::Json);
    }

    #[test]
    fn selects_executable_name_and_tool_exec_before_moon_cli() {
        assert!(matches!(
            select(args(&["moonx", "user/module"])).unwrap(),
            SelectedInvocation::Moonx(_)
        ));
        assert!(matches!(
            select(args(&["moon", "tool", "exec", "--", "echo", "ok"])).unwrap(),
            SelectedInvocation::Delegate(DelegatedInvocation::ToolExec(_))
        ));
    }

    #[test]
    fn selects_ide_help_with_effective_directory() {
        let SelectedInvocation::Delegate(DelegatedInvocation::IdeHelp {
            current_dir,
            args: delegated,
        }) = select(args(&["moon", "-Csub", "help", "ide", "doc"])).unwrap()
        else {
            panic!("help ide should select delegation")
        };
        assert_eq!(current_dir, Some(PathBuf::from("sub")));
        assert_eq!(delegated, args(&["doc", "--help"]));
    }

    #[test]
    fn owns_trace_only_before_the_delegation_point() {
        let error = select(args(&["moon", "--trace", "cram", "--version"]))
            .expect_err("Moon-owned trace should conflict with transparent delegation");
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);

        let SelectedInvocation::Delegate(DelegatedInvocation::Cram {
            args: delegated, ..
        }) = select(args(&["moon", "cram", "--trace"])).unwrap()
        else {
            panic!("trace after cram should belong to moon-cram")
        };
        assert_eq!(delegated, args(&["--trace"]));
    }

    #[test]
    fn keeps_builtin_cram_test_parse_errors_in_moon() {
        let error = select(args(&["moon", "cram", "test", "--target=wasm-gc"]))
            .expect_err("invalid wrapper target should remain a Moon parse error");
        assert_eq!(error.kind(), ErrorKind::InvalidValue);
    }

    #[test]
    fn preserves_cram_parent_help() {
        let error = select(args(&["moon", "cram", "--help"]))
            .expect_err("parent help should remain Clap-rendered wrapper help");
        assert_eq!(error.kind(), ErrorKind::DisplayHelp);
    }

    #[test]
    fn preserves_global_version_before_cram_compatibility_syntax() {
        let SelectedInvocation::Moon(invocation) =
            select(args(&["moon", "--version", "cram", "show"])).unwrap()
        else {
            panic!("a successfully parsed global version should not use the cram fallback")
        };
        assert!(matches!(
            invocation.command,
            MoonBuildSubcommands::Version(_)
        ));
    }
}
