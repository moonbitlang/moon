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
use moonutil::command_output::CommandOutput;

/// Search for modules in the package registry
#[derive(Debug, clap::Parser)]
pub(crate) struct SearchSubcommand {
    /// The keyword to search for
    pub keyword: String,

    /// Limit the number of search results
    #[clap(short, long, default_value_t = 20)]
    pub limit: u32,
}

pub(crate) fn run_search(cmd: SearchSubcommand, output: &CommandOutput) -> anyhow::Result<i32> {
    let results = RegistryClient::configured().search(&cmd.keyword, cmd.limit)?;
    output.write_result(|writer| render_search_results(writer, &results))?;
    Ok(0)
}

fn render_search_results(
    writer: &mut dyn Write,
    results: &[RegistrySearchResult],
) -> std::io::Result<()> {
    for result in results {
        write!(writer, "{}@{}", result.name, result.version)?;
        if let Some(description) = result.description.as_deref() {
            let description = sanitize_registry_description(description);
            if !description.is_empty() {
                write!(writer, ": {description}")?;
            }
        }
        writeln!(writer)?;
    }
    Ok(())
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
        if character.is_whitespace() || character.is_control() {
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
    fn renders_search_results_as_addable_module_coordinates() {
        let mut output = Vec::new();
        render_search_results(
            &mut output,
            &[
                RegistrySearchResult {
                    name: "mizchi/jq".to_owned(),
                    version: Version::new(0, 2, 2),
                    description: Some(
                        "A jq clone\nfor MoonBit\r\x1b[31mwith color\x1b[0m\tand spaces".to_owned(),
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

        expect_test::expect![[r#"
            mizchi/jq@0.2.2: A jq clone for MoonBit with color and spaces
            example/no-description@1.0.0
            example/empty-description@2.0.0
        "#]]
        .assert_eq(&String::from_utf8(output).unwrap());
    }
}
