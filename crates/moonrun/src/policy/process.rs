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

use std::ffi::OsStr;
#[cfg(unix)]
use std::ffi::OsString;

use anyhow::{bail, ensure};

use crate::async_host::AsyncHostResult;

use super::{
    config::{ProcessConfig, ProcessRuleConfig},
    sandbox_denied,
};

#[derive(Clone, Debug)]
pub(crate) enum ProcessPolicy {
    AllowAll,
    Scoped(Vec<ProcessRule>),
}

#[derive(Clone, Debug)]
pub(crate) struct ProcessRule {
    #[cfg(unix)]
    program: String,
    #[cfg(unix)]
    args_prefix: Vec<String>,
    #[cfg(windows)]
    windows_command_line_prefix: Vec<u16>,
}

impl ProcessPolicy {
    pub(super) fn from_config(config: ProcessConfig) -> anyhow::Result<Self> {
        if config.spawn && !config.allow.is_empty() {
            bail!("process.spawn and process.allow cannot be used together");
        }
        if config.spawn {
            return Ok(Self::AllowAll);
        }

        let rules = config
            .allow
            .into_iter()
            .enumerate()
            .map(|(index, rule)| {
                ProcessRule::from_config(rule)
                    .map_err(|error| error.context(format!("invalid process.allow[{index}]")))
            })
            .collect::<anyhow::Result<_>>()?;
        Ok(Self::Scoped(rules))
    }

    #[cfg(unix)]
    pub(crate) fn allows_unix(&self, program: &OsStr, argv: &[OsString]) -> AsyncHostResult<()> {
        match self {
            Self::AllowAll => Ok(()),
            Self::Scoped(rules) if rules.iter().any(|rule| rule.matches_unix(program, argv)) => {
                Ok(())
            }
            Self::Scoped(_) => sandbox_denied("process spawn", None),
        }
    }

    #[cfg(windows)]
    pub(crate) fn allows_windows(&self, command_line: &OsStr) -> AsyncHostResult<()> {
        use std::os::windows::ffi::OsStrExt;

        match self {
            Self::AllowAll => Ok(()),
            Self::Scoped(rules) => {
                let command_line = command_line.encode_wide().collect::<Vec<_>>();
                if rules.iter().any(|rule| {
                    matches_windows_command_line_prefix(
                        &command_line,
                        &rule.windows_command_line_prefix,
                    )
                }) {
                    Ok(())
                } else {
                    sandbox_denied("process spawn", None)
                }
            }
        }
    }
}

impl ProcessRule {
    fn from_config(config: ProcessRuleConfig) -> anyhow::Result<Self> {
        ensure!(!config.program.is_empty(), "program must not be empty");
        ensure!(
            !config.program.contains('\0'),
            "program must not contain NUL"
        );
        ensure!(
            config.args_prefix.iter().all(|arg| !arg.contains('\0')),
            "args_prefix must not contain NUL"
        );

        #[cfg(windows)]
        let windows_command_line_prefix =
            encode_windows_command_line_prefix(&config.program, &config.args_prefix);

        Ok(Self {
            #[cfg(unix)]
            program: config.program,
            #[cfg(unix)]
            args_prefix: config.args_prefix,
            #[cfg(windows)]
            windows_command_line_prefix,
        })
    }

    #[cfg(unix)]
    fn matches_unix(&self, program: &OsStr, argv: &[OsString]) -> bool {
        program == OsStr::new(&self.program)
            && argv.first().is_some_and(|argv0| argv0 == program)
            && argv.get(1..).is_some_and(|args| {
                args.len() >= self.args_prefix.len()
                    && args
                        .iter()
                        .zip(&self.args_prefix)
                        .all(|(actual, expected)| actual == OsStr::new(expected))
            })
    }
}

// Keep this byte-for-byte aligned with
// moonbitlang/async/src/process/windows.mbt::write_arg_with_windows_escape.
#[cfg(any(windows, test))]
fn encode_windows_command_line_prefix(program: &str, args_prefix: &[String]) -> Vec<u16> {
    let mut prefix = Vec::new();
    let mut normalized_program = program.to_owned();
    let extension = normalized_program
        .as_bytes()
        .get(normalized_program.len().saturating_sub(4)..);
    if !extension.is_some_and(|extension| {
        extension.eq_ignore_ascii_case(b".exe") || extension.eq_ignore_ascii_case(b".com")
    }) {
        normalized_program.push_str(".exe");
    }
    write_windows_arg(&mut prefix, &normalized_program);
    for arg in args_prefix {
        prefix.push(b' ' as u16);
        write_windows_arg(&mut prefix, arg);
    }
    prefix
}

#[cfg(any(windows, test))]
fn write_windows_arg(output: &mut Vec<u16>, arg: &str) {
    let arg = arg.encode_utf16().collect::<Vec<_>>();
    if arg.is_empty() {
        output.extend([b'"' as u16, b'"' as u16]);
        return;
    }
    if !arg.iter().any(|unit| {
        *unit == b' ' as u16 || *unit == b'\t' as u16 || *unit == b'"' as u16 || *unit == 0
    }) {
        output.extend(arg);
        return;
    }

    output.push(b'"' as u16);
    let mut backslashes = 0;
    for unit in arg {
        match unit {
            unit if unit == b'\\' as u16 => backslashes += 1,
            unit if unit == b'"' as u16 => {
                output.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2 + 1));
                output.push(unit);
                backslashes = 0;
            }
            unit => {
                output.extend(std::iter::repeat_n(b'\\' as u16, backslashes));
                output.push(unit);
                backslashes = 0;
            }
        }
    }
    output.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2));
    output.push(b'"' as u16);
}

#[cfg(any(windows, test))]
fn matches_windows_command_line_prefix(command_line: &[u16], prefix: &[u16]) -> bool {
    command_line == prefix
        || command_line
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.first() == Some(&(b' ' as u16)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::async_host::AsyncHostError;

    #[cfg(unix)]
    fn scoped_rule(program: &str, args_prefix: &[&str]) -> ProcessPolicy {
        ProcessPolicy::from_config(ProcessConfig {
            spawn: false,
            allow: vec![ProcessRuleConfig {
                program: program.to_owned(),
                args_prefix: args_prefix.iter().map(|arg| (*arg).to_owned()).collect(),
            }],
        })
        .unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn unix_rules_match_program_and_argument_tokens() {
        let policy = scoped_rule("git", &["status"]);

        policy
            .allows_unix(
                OsStr::new("git"),
                &["git", "status", "--short"]
                    .into_iter()
                    .map(OsString::from)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        assert_eq!(
            policy.allows_unix(
                OsStr::new("git"),
                &["git"].into_iter().map(OsString::from).collect::<Vec<_>>(),
            ),
            Err(AsyncHostError::PermissionDenied)
        );
        assert_eq!(
            policy.allows_unix(
                OsStr::new("git"),
                &["git", "status-long"]
                    .into_iter()
                    .map(OsString::from)
                    .collect::<Vec<_>>(),
            ),
            Err(AsyncHostError::PermissionDenied)
        );
        assert_eq!(
            policy.allows_unix(
                OsStr::new("git-other"),
                &["git-other", "status"]
                    .into_iter()
                    .map(OsString::from)
                    .collect::<Vec<_>>(),
            ),
            Err(AsyncHostError::PermissionDenied)
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_rules_reject_noncanonical_argv0() {
        let policy = scoped_rule("git", &["status"]);

        assert_eq!(
            policy.allows_unix(
                OsStr::new("git"),
                &["not-git", "status"]
                    .into_iter()
                    .map(OsString::from)
                    .collect::<Vec<_>>(),
            ),
            Err(AsyncHostError::PermissionDenied)
        );
    }

    #[test]
    fn rejects_ambiguous_process_config() {
        let error = ProcessPolicy::from_config(ProcessConfig {
            spawn: true,
            allow: vec![ProcessRuleConfig {
                program: "git".to_owned(),
                args_prefix: vec!["status".to_owned()],
            }],
        })
        .unwrap_err();
        assert!(error.to_string().contains("cannot be used together"));
    }

    #[cfg(unix)]
    #[test]
    fn empty_argument_prefix_allows_any_arguments_for_the_program() {
        let policy = scoped_rule("git", &[]);

        policy
            .allows_unix(
                OsStr::new("git"),
                &["git", "status", "--short"]
                    .into_iter()
                    .map(OsString::from)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        policy
            .allows_unix(
                OsStr::new("git"),
                &["git"].into_iter().map(OsString::from).collect::<Vec<_>>(),
            )
            .unwrap();
        assert_eq!(
            policy.allows_unix(
                OsStr::new("git-other"),
                &["git-other", "status"]
                    .into_iter()
                    .map(OsString::from)
                    .collect::<Vec<_>>(),
            ),
            Err(AsyncHostError::PermissionDenied)
        );
    }

    #[cfg(unix)]
    #[test]
    fn empty_string_prefix_matches_one_empty_argument_token() {
        let policy = scoped_rule("git", &[""]);

        policy
            .allows_unix(
                OsStr::new("git"),
                &["git", ""]
                    .into_iter()
                    .map(OsString::from)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        assert_eq!(
            policy.allows_unix(
                OsStr::new("git"),
                &["git"].into_iter().map(OsString::from).collect::<Vec<_>>(),
            ),
            Err(AsyncHostError::PermissionDenied)
        );
    }

    #[test]
    fn rejects_invalid_process_rules() {
        for (program, args_prefix, message) in [
            ("", vec![], "program must not be empty"),
            ("git\0other", vec![], "program must not contain NUL"),
            (
                "git",
                vec!["status\0other"],
                "args_prefix must not contain NUL",
            ),
        ] {
            let error = ProcessPolicy::from_config(ProcessConfig {
                spawn: false,
                allow: vec![ProcessRuleConfig {
                    program: program.to_owned(),
                    args_prefix: args_prefix.into_iter().map(str::to_owned).collect(),
                }],
            })
            .unwrap_err();
            assert!(format!("{error:#}").contains(message));
        }
    }

    #[test]
    fn windows_prefix_uses_native_extension_and_escaping_rules() {
        let args = ["status".to_owned(), "a b\\".to_owned()];

        assert_eq!(
            String::from_utf16(&encode_windows_command_line_prefix("git", &args)).unwrap(),
            r#"git.exe status "a b\\""#
        );
        assert_eq!(
            String::from_utf16(&encode_windows_command_line_prefix("GIT.COM", &[])).unwrap(),
            "GIT.COM"
        );
    }

    #[test]
    fn windows_prefix_requires_a_token_boundary() {
        let prefix = "git.exe status".encode_utf16().collect::<Vec<_>>();

        assert!(matches_windows_command_line_prefix(&prefix, &prefix));
        assert!(matches_windows_command_line_prefix(
            &"git.exe status --short".encode_utf16().collect::<Vec<_>>(),
            &prefix,
        ));
        assert!(!matches_windows_command_line_prefix(
            &"git.exe status-long".encode_utf16().collect::<Vec<_>>(),
            &prefix,
        ));
        assert!(!matches_windows_command_line_prefix(
            &"git.exe status\" --short"
                .encode_utf16()
                .collect::<Vec<_>>(),
            &prefix,
        ));
    }

    #[test]
    fn windows_empty_argument_prefix_allows_any_arguments_for_the_program() {
        let prefix = encode_windows_command_line_prefix("git", &[]);

        assert!(matches_windows_command_line_prefix(
            &"git.exe".encode_utf16().collect::<Vec<_>>(),
            &prefix,
        ));
        assert!(matches_windows_command_line_prefix(
            &"git.exe status --short".encode_utf16().collect::<Vec<_>>(),
            &prefix,
        ));
        assert!(!matches_windows_command_line_prefix(
            &"git.exe-other status".encode_utf16().collect::<Vec<_>>(),
            &prefix,
        ));
    }

    #[test]
    fn windows_empty_string_prefix_matches_one_empty_argument_token() {
        let prefix = encode_windows_command_line_prefix("git", &[String::new()]);

        assert!(matches_windows_command_line_prefix(
            &r#"git.exe """#.encode_utf16().collect::<Vec<_>>(),
            &prefix,
        ));
        assert!(matches_windows_command_line_prefix(
            &r#"git.exe "" status"#.encode_utf16().collect::<Vec<_>>(),
            &prefix,
        ));
        assert!(!matches_windows_command_line_prefix(
            &"git.exe".encode_utf16().collect::<Vec<_>>(),
            &prefix,
        ));
        assert!(!matches_windows_command_line_prefix(
            &"git.exe status".encode_utf16().collect::<Vec<_>>(),
            &prefix,
        ));
    }
}
