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

use std::io::Write;

use mooncake::registry::{RegistryClient, RegistrySearchResult};
use moonutil::{
    command_output::CommandOutput,
    user_log::{UserLogCapture, UserLogEntry, UserLogEntryLevel},
};
use serde::Serialize;
use unicode_width::UnicodeWidthStr;

use super::invocation::{JsonCommand, JsonCommandOutcome};

const SEARCH_JSON_ERROR_EXIT_CODE: i32 = -1;

/// Search for modules in the package registry
#[derive(Debug, clap::Parser)]
pub(crate) struct SearchSubcommand {
    /// The keyword to search for
    pub keyword: String,

    /// Limit the number of search results
    #[clap(short, long, default_value_t = 20)]
    pub limit: u32,

    /// Print search results as JSON
    #[clap(long)]
    pub json: bool,
}

pub(crate) fn run_search(cmd: SearchSubcommand, output: &CommandOutput) -> anyhow::Result<i32> {
    let results = RegistryClient::configured().search(&cmd.keyword, cmd.limit)?;
    output.write_result(|writer| render_search_results(writer, &results))?;
    Ok(0)
}

struct SearchJsonOutcome {
    exit_code: i32,
    results: Vec<RegistrySearchResult>,
    error: Option<String>,
}

impl SearchJsonOutcome {
    fn from_error(exit_code: i32, error: impl std::fmt::Display) -> Self {
        Self {
            exit_code,
            results: Vec::new(),
            error: Some(error.to_string()),
        }
    }

    fn exit_code(&self) -> i32 {
        self.exit_code
    }
}

#[derive(Serialize)]
struct SearchJsonReport {
    version: u8,
    status: &'static str,
    results: Vec<RegistrySearchResult>,
    messages: Vec<UserLogEntry>,
}

fn run_search_json(cmd: &SearchSubcommand) -> SearchJsonOutcome {
    match RegistryClient::configured().search(&cmd.keyword, cmd.limit) {
        Ok(results) => SearchJsonOutcome {
            exit_code: 0,
            results,
            error: None,
        },
        Err(error) => {
            SearchJsonOutcome::from_error(SEARCH_JSON_ERROR_EXIT_CODE, format!("{error:#}"))
        }
    }
}

#[derive(Debug)]
struct SearchJsonCommand {
    command: SearchSubcommand,
}

pub(crate) fn json_command(command: SearchSubcommand) -> Box<dyn JsonCommand> {
    Box::new(SearchJsonCommand { command })
}

impl JsonCommand for SearchJsonCommand {
    fn run(
        &self,
        _flags: &moonutil::cli_support::UniversalFlags,
        _output: &CommandOutput,
    ) -> JsonCommandOutcome {
        search_json_outcome(run_search_json(&self.command))
    }

    fn bootstrap_error(&self, message: String) -> JsonCommandOutcome {
        search_json_outcome(SearchJsonOutcome::from_error(
            SEARCH_JSON_ERROR_EXIT_CODE,
            message,
        ))
    }
}

fn search_json_outcome(outcome: SearchJsonOutcome) -> JsonCommandOutcome {
    let exit_code = outcome.exit_code();
    JsonCommandOutcome::new(exit_code, move |output, capture| {
        write_search_json(output, capture, outcome)
    })
}

fn write_search_json(
    output: &CommandOutput,
    capture: &UserLogCapture,
    outcome: SearchJsonOutcome,
) -> anyhow::Result<()> {
    let status = if outcome.exit_code == 0 {
        "success"
    } else {
        "failure"
    };
    let mut messages = capture.take();
    if let Some(error) = outcome.error {
        messages.push(UserLogEntry {
            level: UserLogEntryLevel::Error,
            message: error,
        });
    }
    let report = SearchJsonReport {
        version: 1,
        status,
        results: outcome.results,
        messages,
    };
    output.write_result(|writer| -> anyhow::Result<()> {
        serde_json::to_writer(&mut *writer, &report)?;
        writeln!(writer)?;
        Ok(())
    })
}

fn render_search_results(
    writer: &mut dyn Write,
    results: &[RegistrySearchResult],
) -> std::io::Result<()> {
    // Validate every name before writing anything so one malformed registry
    // result cannot leave a partial, trusted-looking list of coordinates.
    if results
        .iter()
        .any(|result| !is_safe_registry_module_name(&result.name))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "registry search response contains an invalid module name",
        ));
    }

    if results.is_empty() {
        writeln!(writer, "No modules found.")?;
        return Ok(());
    }

    let module_width = results
        .iter()
        .map(|result| result.name.width())
        .max()
        .unwrap_or_default()
        .max("MODULE".len());
    let version_width = results
        .iter()
        .map(|result| result.version.to_string().len())
        .max()
        .unwrap_or_default()
        .max("VERSION".len());
    writeln!(
        writer,
        "{} {} found\n",
        results.len(),
        if results.len() == 1 {
            "module"
        } else {
            "modules"
        }
    )?;
    writeln!(
        writer,
        "{:<module_width$}  {:<version_width$}  DESCRIPTION",
        "MODULE", "VERSION"
    )?;
    for result in results {
        let module_padding = module_width.saturating_sub(result.name.width());
        let description = result
            .description
            .as_deref()
            .map(sanitize_registry_description)
            .filter(|description| !description.is_empty());
        writeln!(
            writer,
            "{}{:module_padding$}  {:<version_width$}  {}",
            result.name,
            "",
            result.version,
            description.as_deref().unwrap_or("—")
        )?;
    }
    writeln!(
        writer,
        "\nRun `moon add <module>@<version>` to add a dependency."
    )?;
    Ok(())
}

fn is_safe_registry_module_name(name: &str) -> bool {
    name.split('/').all(|component| {
        !component.is_empty()
            && component != "."
            && component != ".."
            && !component.contains(['@', ':', '\\'])
            && component.chars().all(|character| {
                !character.is_whitespace()
                    && !character.is_control()
                    && !is_bidirectional_format_control(character)
            })
    })
}

fn is_bidirectional_format_control(character: char) -> bool {
    // These Unicode formatting controls can reorder surrounding text without
    // occupying a visible cell, making a coordinate appear to say something else.
    matches!(
        character,
        '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{206f}'
    )
}

fn sanitize_registry_description(description: &str) -> String {
    // Preserve word boundaries across lines before stripping terminal escape
    // sequences, which would otherwise discard newlines along with controls.
    let single_line = description
        .chars()
        .map(|character| {
            if character.is_whitespace() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let printable = anstream::adapter::strip_str(&single_line).to_string();

    let mut sanitized = String::with_capacity(printable.len());
    let mut pending_space = false;
    for character in printable.chars() {
        if character.is_whitespace()
            || character.is_control()
            || is_bidirectional_format_control(character)
        {
            pending_space = !sanitized.is_empty();
        } else {
            if pending_space {
                sanitized.push(' ');
                pending_space = false;
            }
            sanitized.push(character);
        }
    }
    sanitized
}

#[cfg(test)]
mod tests {
    use semver::Version;

    use super::*;

    #[test]
    fn renders_search_results_as_a_table() {
        let mut output = Vec::new();
        render_search_results(
            &mut output,
            &[
                RegistrySearchResult {
                    name: "mizchi/jq".to_owned(),
                    version: Version::new(0, 2, 2),
                    description: Some(
                        "A jq clone\nfor MoonBit\r\x1b[31mwith color\x1b[0m\tand\u{202e}spaces"
                            .to_owned(),
                    ),
                },
                RegistrySearchResult {
                    name: "example/no-description".to_owned(),
                    version: Version::new(1, 0, 0),
                    description: None,
                },
                RegistrySearchResult {
                    name: "example/empty-description".to_owned(),
                    version: Version::new(2, 0, 0),
                    description: Some("\x1b[2J\r\n".to_owned()),
                },
            ],
        )
        .unwrap();

        let output = String::from_utf8(output).unwrap();

        expect_test::expect![[r#"
            3 modules found

            MODULE                     VERSION  DESCRIPTION
            mizchi/jq                  0.2.2    A jq clone for MoonBit with color and spaces
            example/no-description     1.0.0    —
            example/empty-description  2.0.0    —

            Run `moon add <module>@<version>` to add a dependency.
        "#]]
        .assert_eq(&output);
    }

    #[test]
    fn aligns_unicode_module_names_by_display_width() {
        let mut output = Vec::new();
        render_search_results(
            &mut output,
            &[
                RegistrySearchResult {
                    name: "example/ascii".to_owned(),
                    version: Version::new(1, 0, 0),
                    description: None,
                },
                RegistrySearchResult {
                    name: "example/中".to_owned(),
                    version: Version::new(2, 0, 0),
                    description: None,
                },
                RegistrySearchResult {
                    name: "example/e\u{301}".to_owned(),
                    version: Version::new(3, 0, 0),
                    description: None,
                },
            ],
        )
        .unwrap();

        let output = String::from_utf8(output).unwrap();
        let version_columns = [
            ("example/ascii", "1.0.0"),
            ("example/中", "2.0.0"),
            ("example/e\u{301}", "3.0.0"),
        ]
        .map(|(name, version)| {
            let row = output
                .lines()
                .find(|line| line.starts_with(name))
                .expect("module row should be present");
            let version_start = row.find(version).expect("version should be present");
            unicode_width::UnicodeWidthStr::width(&row[..version_start])
        });

        assert_eq!(version_columns, [15, 15, 15]);
    }

    #[test]
    fn renders_an_explicit_empty_state() {
        let mut output = Vec::new();
        render_search_results(&mut output, &[]).unwrap();

        expect_test::expect![[r#"
            No modules found.
        "#]]
        .assert_eq(&String::from_utf8(output).unwrap());
    }

    #[test]
    fn rejects_unsafe_module_names_before_rendering() {
        for name in [
            "example/module\nforged/coordinate",
            "example/module\roverwrite",
            "example/\x1b[2Jmodule",
            "example/\u{202e}module",
            "example/\u{2066}module",
            "example/module name",
            "example/module@9.9.9",
        ] {
            let mut output = Vec::new();
            let error = render_search_results(
                &mut output,
                &[
                    RegistrySearchResult {
                        name: "example/valid".to_owned(),
                        version: Version::new(1, 0, 0),
                        description: None,
                    },
                    RegistrySearchResult {
                        name: name.to_owned(),
                        version: Version::new(2, 0, 0),
                        description: None,
                    },
                ],
            )
            .unwrap_err();

            assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
            assert_eq!(
                error.to_string(),
                "registry search response contains an invalid module name"
            );
            assert!(output.is_empty());
        }
    }
}
