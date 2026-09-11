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

use mooncake::registry::{RegistryClient, RegistryPackageMatch, RegistrySearchResult};
use moonutil::{
    command_output::CommandOutput,
    user_log::{UserLogCapture, UserLogEntry, UserLogEntryLevel},
};
use serde::Serialize;

use super::invocation::{JsonCommand, JsonCommandOutcome};

const SEARCH_JSON_ERROR_EXIT_CODE: i32 = -1;

/// Search modules and package summaries in the registry
///
/// Results follow the registry's ranking, as on mooncakes.io (most downloaded
/// first). Each module includes its version, description, download count, and
/// matching package excerpts when available. Summaries from older versions are
/// labeled, and a count indicates when only some matching packages are shown.
///
/// With --json, the result also preserves the registry's summary fragments and
/// match markers. Registries without package summaries remain supported.
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
    for result in results {
        if !is_safe_registry_module_name(&result.name) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "registry search response contains an invalid module name",
            ));
        }
        if result
            .matched_packages
            .iter()
            .any(|package| !is_safe_registry_module_name(&package.name))
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "registry search response contains an invalid package name",
            ));
        }
    }

    if results.is_empty() {
        writeln!(writer, "No modules found.")?;
        return Ok(());
    }

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
    let heading = anstyle::Style::new().bold();
    for result in results {
        write!(
            writer,
            "{heading}{}@{}{heading:#}",
            result.name, result.version
        )?;
        if let Some(downloads) = result.downloads {
            write!(writer, " ({downloads} downloads)")?;
        }
        writeln!(writer)?;
        let description = result
            .description
            .as_deref()
            .map(sanitize_registry_text)
            .filter(|description| !description.is_empty());
        writeln!(writer, "  {}", description.as_deref().unwrap_or("—"))?;
        for package in &result.matched_packages {
            writeln!(writer)?;
            write!(writer, "  {}", package.name)?;
            if !package.is_summary_current {
                write!(
                    writer,
                    " (summary from v{})",
                    sanitize_registry_text(&package.summary_version)
                )?;
            }
            writeln!(writer)?;
            write!(writer, "    ")?;
            render_package_summary(writer, package)?;
            writeln!(writer)?;
        }
        let shown = result.matched_packages.len();
        if let Some(total) = result.matched_package_count.filter(|&total| total > shown) {
            writeln!(writer, "  Showing {shown} of {total} matching packages.")?;
        }
        writeln!(writer)?;
    }
    writeln!(
        writer,
        "Run `moon add <module>@<version>` to add a dependency."
    )?;
    Ok(())
}

pub(super) fn is_safe_registry_module_name(name: &str) -> bool {
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

fn render_package_summary(
    writer: &mut dyn Write,
    package: &RegistryPackageMatch,
) -> std::io::Result<()> {
    let fragments = package
        .summary_fragments
        .iter()
        .map(|fragment| (fragment.text.as_str(), fragment.matched))
        .chain(
            package
                .summary_fragments
                .is_empty()
                .then_some((package.summary.as_str(), false)),
        );

    // Keep parser state across fragments so registry escape sequences cannot
    // bypass filtering by spanning fragment boundaries. Add styling afterwards.
    let mut strip = anstream::adapter::StripBytes::new();
    let highlight = anstyle::Style::new().bold();
    let mut has_text = false;
    let mut pending_space = false;
    let mut pending_newlines = 0;
    for (fragment, matched) in fragments {
        let mut text = String::new();
        let printable = strip
            .strip_next(fragment.as_bytes())
            .flatten()
            .copied()
            .collect();
        let printable = String::from_utf8(printable)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        for character in printable.chars() {
            if character == '\n' {
                if has_text {
                    pending_newlines += 1;
                }
            } else if character.is_whitespace()
                || character.is_control()
                || is_bidirectional_format_control(character)
            {
                pending_space = has_text;
            } else {
                if pending_newlines > 0 {
                    // Only preserve explicit line breaks; the terminal owns
                    // wrapping. Empty lines have no trailing indentation.
                    text.extend(std::iter::repeat_n('\n', pending_newlines));
                    text.push_str("    ");
                } else if pending_space {
                    text.push(' ');
                }
                pending_newlines = 0;
                pending_space = false;
                text.push(character);
                has_text = true;
            }
        }
        if matched && !text.is_empty() {
            write!(writer, "{highlight}{text}{highlight:#}")?;
        } else {
            write!(writer, "{text}")?;
        }
    }
    Ok(())
}

pub(super) fn sanitize_registry_text(description: &str) -> String {
    // Keep descriptions and version labels on one line while preserving word
    // boundaries and stripping terminal escape sequences.
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
    fn renders_package_matches_in_registry_order() {
        let results: Vec<RegistrySearchResult> = serde_json::from_value(serde_json::json!([
            {
                "name": "z/tools", "version": "2.0.0", "downloads": 100,
                "matched_package_count": 7,
                "matched_packages": [
                    {
                        "package": "fs", "name": "z/tools/fs",
                        "summary": "Full summary omitted by the excerpt",
                        "summary_version": "1.0.0\u{1b}[2J\u{202e}",
                        "is_summary_current": false,
                        "summary_fragments": [
                            {"text": "  Read\r", "matched": false},
                            {"text": "\n", "matched": false},
                            {"text": "files", "matched": true},
                            {"text": "\t safely.\n\nKeywords: 文件, IO  ", "matched": false}
                        ]
                    },
                    {
                        "package": "", "name": "z/tools",
                        "summary": "Root\n\nsummary\u{1b}[31m.\u{1b}[0m",
                        "summary_version": "2.0.0", "is_summary_current": true
                    }
                ]
            },
            {"name": "a/tools", "version": "3.0.0", "downloads": 1000}
        ]))
        .unwrap();
        let mut output = Vec::new();
        render_search_results(&mut output, &results).unwrap();
        let output = String::from_utf8(output).unwrap();
        let plain = anstream::adapter::strip_str(&output).to_string();
        expect_test::expect![[r#"
            2 modules found

            z/tools@2.0.0 (100 downloads)
              —

              z/tools/fs (summary from v1.0.0)
                Read
                files safely.

                Keywords: 文件, IO

              z/tools
                Root

                summary.
              Showing 2 of 7 matching packages.

            a/tools@3.0.0 (1000 downloads)
              —

            Run `moon add <module>@<version>` to add a dependency.
        "#]]
        .assert_eq(&plain);
        assert!(output.contains("\u{1b}[1mz/tools@2.0.0\u{1b}[0m"));
        assert!(output.contains("\u{1b}[1m\n    files\u{1b}[0m"));
    }

    #[test]
    fn sanitizes_terminal_controls_across_summary_fragments() {
        let package: RegistryPackageMatch = serde_json::from_value(serde_json::json!({
            "package": "fs", "name": "alice/tools/fs",
            "summary": "Unused full summary",
            "summary_version": "1.0.0", "is_summary_current": true,
            "summary_fragments": [
                {"text": "Read \u{1b}[", "matched": false},
                {"text": "2Jfiles\u{1b}]8;;https://example.com", "matched": true},
                {"text": "\u{7} here\u{1b}]8;;\u{7}\u{202e} safely", "matched": false}
            ]
        }))
        .unwrap();
        let mut output = Vec::new();
        render_package_summary(&mut output, &package).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "Read\u{1b}[1m files\u{1b}[0m here safely"
        );
    }

    #[test]
    fn rejects_unsafe_package_names_before_rendering_any_results() {
        let results: Vec<RegistrySearchResult> = serde_json::from_value(serde_json::json!([
            {"name": "alice/valid", "version": "1.0.0"},
            {
                "name": "alice/tools", "version": "1.0.0",
                "matched_packages": [{
                    "package": "fs", "name": "alice/tools/fs\nforged/coordinate",
                    "summary": "Read files", "summary_version": "1.0.0",
                    "is_summary_current": true
                }]
            }
        ]))
        .unwrap();
        let mut output = Vec::new();
        let error = render_search_results(&mut output, &results).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(
            error.to_string(),
            "registry search response contains an invalid package name"
        );
        assert!(output.is_empty());
    }

    #[test]
    fn renders_legacy_search_results() {
        let mut output = Vec::new();
        render_search_results(
            &mut output,
            &[
                RegistrySearchResult {
                    name: "mizchi/jq".to_owned(),
                    version: Version::new(0, 2, 2),
                    downloads: None,
                    matched_package_count: None,
                    matched_packages: Vec::new(),
                    description: Some(
                        "A jq clone\nfor MoonBit\r\x1b[31mwith color\x1b[0m\tand\u{202e}spaces"
                            .to_owned(),
                    ),
                },
                RegistrySearchResult {
                    name: "example/no-description".to_owned(),
                    version: Version::new(1, 0, 0),
                    downloads: None,
                    matched_package_count: None,
                    matched_packages: Vec::new(),
                    description: None,
                },
                RegistrySearchResult {
                    name: "example/empty-description".to_owned(),
                    version: Version::new(2, 0, 0),
                    downloads: None,
                    matched_package_count: None,
                    matched_packages: Vec::new(),
                    description: Some("\x1b[2J\r\n".to_owned()),
                },
            ],
        )
        .unwrap();

        let output = String::from_utf8(output).unwrap();
        let output = anstream::adapter::strip_str(&output).to_string();

        expect_test::expect![[r#"
            3 modules found

            mizchi/jq@0.2.2
              A jq clone for MoonBit with color and spaces

            example/no-description@1.0.0
              —

            example/empty-description@2.0.0
              —

            Run `moon add <module>@<version>` to add a dependency.
        "#]]
        .assert_eq(&output);
    }

    #[test]
    fn preserves_unicode_module_names() {
        let mut output = Vec::new();
        render_search_results(
            &mut output,
            &[
                RegistrySearchResult {
                    name: "example/ascii".to_owned(),
                    version: Version::new(1, 0, 0),
                    downloads: None,
                    matched_package_count: None,
                    matched_packages: Vec::new(),
                    description: None,
                },
                RegistrySearchResult {
                    name: "example/中".to_owned(),
                    version: Version::new(2, 0, 0),
                    downloads: None,
                    matched_package_count: None,
                    matched_packages: Vec::new(),
                    description: None,
                },
                RegistrySearchResult {
                    name: "example/e\u{301}".to_owned(),
                    version: Version::new(3, 0, 0),
                    downloads: None,
                    matched_package_count: None,
                    matched_packages: Vec::new(),
                    description: None,
                },
            ],
        )
        .unwrap();

        let output = String::from_utf8(output).unwrap();
        let output = anstream::adapter::strip_str(&output).to_string();
        expect_test::expect![[r#"
            3 modules found

            example/ascii@1.0.0
              —

            example/中@2.0.0
              —

            example/é@3.0.0
              —

            Run `moon add <module>@<version>` to add a dependency.
        "#]]
        .assert_eq(&output);
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
                        downloads: None,
                        matched_package_count: None,
                        matched_packages: Vec::new(),
                        description: None,
                    },
                    RegistrySearchResult {
                        name: name.to_owned(),
                        version: Version::new(2, 0, 0),
                        downloads: None,
                        matched_package_count: None,
                        matched_packages: Vec::new(),
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
