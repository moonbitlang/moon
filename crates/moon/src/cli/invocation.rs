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

//! Side-effect-free selection of one top-level invocation.

use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use clap::{CommandFactory, Parser, error::ErrorKind};
use moonutil::{
    cli_support::UniversalFlags, command_output::CommandOutput, user_log::UserLogCapture,
};

use super::{CramCommand, MoonBuildCli, MoonBuildSubcommands, VersionSubcommand, moonx, tool};

#[derive(Debug)]
pub(crate) struct MoonInvocation {
    pub(crate) flags: UniversalFlags,
    pub(crate) command: MoonCommand,
}

#[derive(Debug)]
pub(crate) enum MoonCommand {
    Human(Box<MoonBuildSubcommands>),
    Json(Box<dyn JsonCommand>),
}

/// Command-owned hooks for one machine-readable lifecycle.
pub(crate) trait JsonCommand: std::fmt::Debug {
    fn run(&self, flags: &UniversalFlags, output: &CommandOutput) -> JsonCommandOutcome;

    fn bootstrap_error(&self, message: String) -> JsonCommandOutcome;
}

type JsonFinish =
    Box<dyn FnOnce(&CommandOutput, &UserLogCapture) -> anyhow::Result<()> + Send + 'static>;

pub(crate) struct JsonCommandOutcome {
    exit_code: i32,
    finish: JsonFinish,
}

impl JsonCommandOutcome {
    pub(crate) fn new(
        exit_code: i32,
        finish: impl FnOnce(&CommandOutput, &UserLogCapture) -> anyhow::Result<()> + Send + 'static,
    ) -> Self {
        Self {
            exit_code,
            finish: Box::new(finish),
        }
    }

    pub(crate) fn exit_code(&self) -> i32 {
        self.exit_code
    }

    pub(crate) fn finish(
        self,
        output: &CommandOutput,
        capture: &UserLogCapture,
    ) -> anyhow::Result<()> {
        (self.finish)(output, capture)
    }
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
    TestCli {
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
    ArtifactHandoff(Box<ArtifactHandoffInvocation>),
}

/// Only executable-producing commands can participate in artifact delivery.
#[derive(Debug)]
pub(crate) struct ArtifactHandoffInvocation {
    pub(crate) directory: PathBuf,
    pub(crate) command: ArtifactHandoffCommand,
}

#[derive(Debug)]
pub(crate) enum ArtifactHandoffCommand {
    Run(UniversalFlags, Box<super::run::RunSubcommand>),
    Moonx(moonx::MoonxInvocation),
}

/// Parse and classify an invocation before output, tracing, workspace state,
/// current-directory changes, or child processes are initialized.
pub(crate) fn select(mut raw_args: Vec<OsString>) -> Result<SelectedInvocation, clap::Error> {
    // This private prefix belongs to Moon, before command or program arguments.
    // Parse only its path here; runtime opens it before applying any `-C`.
    if raw_args
        .get(1)
        .is_some_and(|arg| arg == moonutil::run_handoff::HANDOFF_ARG)
    {
        if raw_args.len() < 3 {
            return Err(MoonBuildCli::command().error(
                ErrorKind::MissingRequiredArgument,
                "Moonrun handoff requires a directory",
            ));
        }
        let directory = PathBuf::from(raw_args.remove(2));
        raw_args.remove(1);
        let command = match select_command(raw_args)? {
            SelectedInvocation::Moon(invocation) => match *invocation {
                MoonInvocation {
                    flags,
                    command: MoonCommand::Human(command),
                } => match *command {
                    MoonBuildSubcommands::Run(command) => {
                        ArtifactHandoffCommand::Run(flags, Box::new(command))
                    }
                    _ => return Err(handoff_command_error()),
                },
                _ => return Err(handoff_command_error()),
            },
            SelectedInvocation::Moonx(invocation) => ArtifactHandoffCommand::Moonx(invocation),
            _ => return Err(handoff_command_error()),
        };
        return Ok(SelectedInvocation::ArtifactHandoff(Box::new(
            ArtifactHandoffInvocation { directory, command },
        )));
    }
    select_command(raw_args)
}

fn handoff_command_error() -> clap::Error {
    MoonBuildCli::command().error(
        ErrorKind::ArgumentConflict,
        "Moonrun handoff requires `moon run` or `moonx`",
    )
}

fn select_command(raw_args: Vec<OsString>) -> Result<SelectedInvocation, clap::Error> {
    if moonx::is_moonx_invocation(&raw_args) {
        return moonx::parse_from(&raw_args).map(SelectedInvocation::Moonx);
    }
    if tool::exec::is_tool_exec(&raw_args) {
        return tool::exec::parse_from_raw_args(&raw_args)
            .map(DelegatedInvocation::ToolExec)
            .map(SelectedInvocation::Delegate);
    }
    let external_cram = select_external_cram(&raw_args);
    // Clap validates only the Moon-owned prefix. Ask it to identify the
    // command boundary too, since `testcli` can be an option value or a path
    // under another command. This also preserves non-UTF-8 paths in the prefix.
    let testcli = raw_args
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, arg)| *arg == OsStr::new("testcli"))
        .find_map(|(index, _)| {
            let parsed = MoonBuildCli::try_parse_from(&raw_args[..=index]).ok()?;
            matches!(parsed.subcommand, Some(MoonBuildSubcommands::TestCli))
                .then_some((parsed, &raw_args[index + 1..]))
        });
    let (parsed, testcli_args) = match testcli {
        Some((parsed, tail)) => (Ok(parsed), tail),
        None => (MoonBuildCli::try_parse_from(&raw_args), [].as_slice()),
    };
    let parsed = match parsed {
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
        MoonBuildSubcommands::TestCli => select_transparent_delegate(
            "testcli",
            flags.trace,
            DelegatedInvocation::TestCli {
                current_dir,
                args: testcli_args.to_vec(),
            },
        ),
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
            let command = match command {
                MoonBuildSubcommands::Check(command) if command.json => {
                    MoonCommand::Json(super::check::json_command(command))
                }
                MoonBuildSubcommands::Search(command) if command.json => {
                    MoonCommand::Json(super::search::json_command(command))
                }
                MoonBuildSubcommands::View(command) if command.json => {
                    MoonCommand::Json(super::view::json_command(command))
                }
                MoonBuildSubcommands::Tree(command) if command.json => {
                    MoonCommand::Json(super::tree::json_command(command))
                }
                command => MoonCommand::Human(Box::new(command)),
            };
            Ok(SelectedInvocation::Moon(Box::new(MoonInvocation {
                flags,
                command,
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
            Some("-V" | "--version" | "-q" | "--quiet" | "-v" | "--verbose" | "--dry-run") => {}
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
    fn handoff_parsing_is_pure_and_keeps_the_program_argument_tail() {
        let SelectedInvocation::ArtifactHandoff(invocation) = select(args(&[
            "moon",
            "--moonrun-artifact-handoff",
            "not-created",
            "-C",
            "project",
            "run",
            "script.mbtx",
            "--",
            "--moonrun-artifact-handoff",
            "guest-arg",
        ]))
        .unwrap() else {
            panic!("expected handoff")
        };
        assert_eq!(invocation.directory, PathBuf::from("not-created"));
        let ArtifactHandoffCommand::Run(flags, command) = invocation.command else {
            panic!("expected run")
        };
        assert_eq!(flags.source_tgt_dir.cwd, Some(PathBuf::from("project")));
        assert_eq!(command.args, ["--moonrun-artifact-handoff", "guest-arg"]);
        assert!(command.handoff.is_none());
        assert!(matches!(
            select(args(&[
                "moonx",
                "--moonrun-artifact-handoff",
                "not-created",
                "user/module",
            ]))
            .unwrap(),
            SelectedInvocation::ArtifactHandoff(_)
        ));
        assert_eq!(
            select(args(&[
                "moon",
                "--moonrun-artifact-handoff",
                "not-created",
                "new",
                "project",
            ]))
            .unwrap_err()
            .kind(),
            ErrorKind::ArgumentConflict
        );
        assert_eq!(
            select(args(&["moon", "--moonrun-artifact-handoff",]))
                .unwrap_err()
                .kind(),
            ErrorKind::MissingRequiredArgument
        );
    }

    #[test]
    fn selects_human_and_json_moon_commands() {
        let SelectedInvocation::Moon(build) = select(args(&["moon", "build"])).unwrap() else {
            panic!("build should select a Moon invocation")
        };
        assert!(matches!(build.command, MoonCommand::Human(_)));

        let SelectedInvocation::Moon(check) = select(args(&["moon", "check", "--json"])).unwrap()
        else {
            panic!("JSON check should select a Moon invocation")
        };
        assert!(matches!(check.command, MoonCommand::Json(_)));

        let SelectedInvocation::Moon(search) =
            select(args(&["moon", "search", "json", "--json"])).unwrap()
        else {
            panic!("JSON search should select a Moon invocation")
        };
        assert!(matches!(search.command, MoonCommand::Json(_)));

        let SelectedInvocation::Moon(tree) = select(args(&["moon", "tree"])).unwrap() else {
            panic!("tree should select a Moon invocation")
        };
        assert!(matches!(tree.command, MoonCommand::Human(_)));

        let SelectedInvocation::Moon(tree) = select(args(&["moon", "tree", "--json"])).unwrap()
        else {
            panic!("JSON tree should select a Moon invocation")
        };
        assert!(matches!(tree.command, MoonCommand::Json(_)));
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
    fn testcli_preserves_the_opaque_argument_tail() {
        for tail in [
            vec![],
            args(&["--help"]),
            args(&["-h"]),
            args(&["--version"]),
            args(&["-V"]),
            args(&["--trace", "--quiet", "-q", "--verbose", "-v", "--dry-run"]),
            args(&["-C", "elsewhere", "--target-dir", "output", "-Z", "unknown"]),
            args(&["--unstable-feature=unknown", "--unknown-option", "value"]),
            args(&["--", "", "two words", "--", "--help"]),
        ] {
            let raw = args(&["moon", "testcli"])
                .into_iter()
                .chain(tail.clone())
                .collect();
            let SelectedInvocation::Delegate(DelegatedInvocation::TestCli {
                current_dir,
                args: delegated,
            }) = select(raw).unwrap()
            else {
                panic!("testcli should select transparent delegation for {tail:?}")
            };
            assert_eq!(current_dir, None);
            assert_eq!(delegated, tail);
        }
    }

    #[cfg(unix)]
    #[test]
    fn testcli_preserves_non_utf8_arguments() {
        use std::os::unix::ffi::OsStringExt;

        let tail = vec![OsString::from_vec(b"non-utf8-\xff".to_vec())];
        let raw = args(&["moon", "testcli"])
            .into_iter()
            .chain(tail.clone())
            .collect();
        let SelectedInvocation::Delegate(DelegatedInvocation::TestCli {
            args: delegated, ..
        }) = select(raw).unwrap()
        else {
            panic!("testcli should preserve OS strings")
        };
        assert_eq!(delegated, tail);
    }

    #[cfg(unix)]
    #[test]
    fn testcli_keeps_non_utf8_prefix_paths_in_moon() {
        use std::os::unix::ffi::OsStringExt;

        let directory = OsString::from_vec(b"directory-\xff".to_vec());
        let mut option = OsString::from("-qC");
        option.push(&directory);
        let raw = vec!["moon".into(), option, "testcli".into(), "--help".into()];
        let SelectedInvocation::Delegate(DelegatedInvocation::TestCli {
            current_dir,
            args: delegated,
        }) = select(raw).unwrap()
        else {
            panic!("testcli should preserve OS strings in the Moon prefix")
        };
        assert_eq!(current_dir, Some(PathBuf::from(directory)));
        assert_eq!(delegated, args(&["--help"]));
    }

    #[test]
    fn testcli_validates_only_the_moon_prefix() {
        for prefix in [
            args(&["-C", "subdir"]),
            args(&["-Csubdir"]),
            args(&["-C=subdir"]),
            args(&["-qvCsubdir"]),
            args(&["--target-dir", "testcli", "-C", "subdir"]),
        ] {
            let raw = args(&["moon"])
                .into_iter()
                .chain(prefix)
                .chain(args(&["testcli", "-C", "guest directory", "--trace"]))
                .collect();
            let SelectedInvocation::Delegate(DelegatedInvocation::TestCli {
                current_dir,
                args: delegated,
            }) = select(raw).unwrap()
            else {
                panic!("testcli should select transparent delegation")
            };
            assert_eq!(current_dir, Some(PathBuf::from("subdir")));
            assert_eq!(delegated, args(&["-C", "guest directory", "--trace"]));
        }

        for (raw, kind) in [
            (
                args(&["moon", "--trace", "testcli", "--help"]),
                ErrorKind::ArgumentConflict,
            ),
            (args(&["moon", "--help", "testcli"]), ErrorKind::DisplayHelp),
            (args(&["moon", "help", "testcli"]), ErrorKind::DisplayHelp),
            (
                args(&["moon", "--unknown", "testcli"]),
                ErrorKind::UnknownArgument,
            ),
            (
                args(&["moon", "build", "testcli", "--unknown"]),
                ErrorKind::UnknownArgument,
            ),
        ] {
            assert_eq!(select(raw).unwrap_err().kind(), kind);
        }

        let SelectedInvocation::Moon(invocation) =
            select(args(&["moon", "--version", "testcli", "--unknown"])).unwrap()
        else {
            panic!("version before testcli should belong to Moon")
        };
        assert!(matches!(invocation.command, MoonCommand::Human(command)
            if matches!(*command, MoonBuildSubcommands::Version(_))));
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
        let MoonCommand::Human(command) = invocation.command else {
            panic!("global version should select a human command")
        };
        assert!(matches!(*command, MoonBuildSubcommands::Version(_)));
    }
}
