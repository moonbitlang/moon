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

#[path = "explain_warning_index.rs"]
mod warning_index;

use std::process::{Command, Stdio};

use anyhow::{Context, bail};
use moonutil::error_code_docs::{
    get_all_attribute_docs, get_all_error_code_docs, get_error_code_doc,
};
use moonutil::{cli_support::UniversalFlags, toolchain::BINARIES};

/// Explain compiler diagnostics and language topics.
#[derive(Debug, clap::Parser)]
#[clap(
    arg_required_else_help = true,
    group(
        clap::ArgGroup::new("explain_mode")
            .required(true)
            .args(["diagnostic", "attribute"])
    ),
    after_help = "Resources:\n    Docs: https://docs.moonbitlang.com\n    Skills: https://github.com/moonbitlang/skills\n\n    Use `moon explain --diagnostic` to list diagnostic codes and names.\n    Use `moon explain --attribute` to list attributes."
)]
pub(crate) struct ExplainSubcommand {
    /// Explain diagnostics. Without a query, list diagnostic codes and names.
    #[clap(
        long,
        alias = "diagnostics",
        num_args = 0..=1,
        default_missing_value = "",
        value_name = "ID_OR_NAME"
    )]
    pub diagnostic: Option<String>,

    /// Explain attributes. Without a query, list attribute names.
    #[clap(
        long,
        num_args = 0..=1,
        default_missing_value = "",
        value_name = "NAME"
    )]
    pub attribute: Option<String>,
}

pub(crate) fn run_explain(_cli: &UniversalFlags, cmd: ExplainSubcommand) -> anyhow::Result<i32> {
    if let Some(query) = cmd.diagnostic.as_deref() {
        return match query {
            "" => list_diagnostics(),
            query => explain_diagnostic(query),
        };
    }

    if let Some(query) = cmd.attribute.as_deref() {
        return match query {
            "" => list_attributes(),
            query => explain_attribute(query),
        };
    }

    unreachable!("clap requires an argument for `moon explain`")
}

fn list_diagnostics() -> anyhow::Result<i32> {
    let status = Command::new(&*BINARIES.moonc)
        .arg("check")
        .arg("-warn-help")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to run `{}`", BINARIES.moonc.display()))?;
    if !status.success() {
        bail!("`moonc check -warn-help` failed");
    }

    let entries = non_warning_diagnostic_index_entries();
    if !entries.is_empty() {
        println!();
        println!("Available non-warning diagnostics:");
        for entry in entries {
            println!("  E{}  diagnostic {}", entry.code, entry.names.join(", "));
        }
        println!();
        println!("Use `moon explain --diagnostic <ID_OR_NAME>` to show details.");
    }
    Ok(0)
}

fn explain_diagnostic(query: &str) -> anyhow::Result<i32> {
    let docs = diagnostic_docs(query);
    if docs.is_empty() {
        bail!(
            "no integrated diagnostic docs found for `{}`. Try `moon explain --diagnostic` to list available diagnostics.",
            query.trim()
        );
    }

    for (index, doc) in docs.into_iter().enumerate() {
        if index > 0 {
            println!();
        }
        println!("{doc}");
    }
    Ok(0)
}

fn diagnostic_docs(query: &str) -> Vec<String> {
    let query = query.trim();
    let digits = query
        .strip_prefix('E')
        .or_else(|| query.strip_prefix('e'))
        .unwrap_or(query);
    let diagnostic_id = if !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit()) {
        digits.parse::<u16>().ok()
    } else {
        None
    };

    if let Some(id) = diagnostic_id {
        return match get_error_code_doc(&format!("{id:04}")) {
            Some(doc) => vec![doc.trim_end().to_owned()],
            None => warning_index::get_warning_entry(id)
                .map(|entry| vec![entry.render_markdown()])
                .unwrap_or_default(),
        };
    }

    let docs: Vec<_> = get_all_error_code_docs()
        .into_iter()
        .filter(|(_, doc)| {
            diagnostic_names(doc)
                .1
                .iter()
                .any(|name| diagnostic_name_matches(name, query))
        })
        .map(|(_, doc)| doc.trim_end().to_owned())
        .collect();
    if !docs.is_empty() {
        return docs;
    }

    warning_index::get_warning_entries_by_mnemonic(query)
        .into_iter()
        .map(|entry| {
            // Mnemonics can map to multiple warning IDs. Prefer integrated docs,
            // and fall back to the compiler snapshot when docs have not caught up yet.
            get_error_code_doc(&format!("{:04}", entry.id))
                .map(|doc| doc.trim_end().to_owned())
                .unwrap_or_else(|| entry.render_markdown())
        })
        .collect()
}

fn list_attributes() -> anyhow::Result<i32> {
    let entries = attribute_doc_entries();
    if entries.is_empty() {
        bail!("no integrated attribute docs found. Run `git submodule update --init --recursive`.");
    }

    println!("Available attributes:");
    for entry in entries {
        let names = entry
            .names
            .iter()
            .map(|name| format!("#{name}"))
            .collect::<Vec<_>>()
            .join(", ");
        println!("  {names}");
    }
    println!();
    println!("Use `moon explain --attribute <NAME>` to show details.");
    Ok(0)
}

fn explain_attribute(query: &str) -> anyhow::Result<i32> {
    let docs = attribute_docs(query);
    if docs.is_empty() {
        bail!(
            "no integrated attribute docs found for `{}`. Try `moon explain --attribute` to list available attributes.",
            query.trim()
        );
    }

    for (index, doc) in docs.into_iter().enumerate() {
        if index > 0 {
            println!();
        }
        println!("{doc}");
    }
    Ok(0)
}

fn attribute_docs(query: &str) -> Vec<String> {
    let query = normalize_attribute_name(query);
    attribute_doc_entries()
        .into_iter()
        .filter(|entry| {
            normalize_attribute_name(entry.key) == query
                || entry
                    .names
                    .iter()
                    .any(|name| normalize_attribute_name(name) == query)
        })
        .map(|entry| entry.doc.trim_end().to_owned())
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
struct DiagnosticIndexEntry<'a> {
    code: &'a str,
    kind: Option<&'static str>,
    names: Vec<&'a str>,
}

fn diagnostic_index_entries() -> Vec<DiagnosticIndexEntry<'static>> {
    get_all_error_code_docs()
        .into_iter()
        .map(|(code, doc)| {
            let (kind, names) = diagnostic_names(doc);
            DiagnosticIndexEntry { code, kind, names }
        })
        .collect()
}

fn non_warning_diagnostic_index_entries() -> Vec<DiagnosticIndexEntry<'static>> {
    diagnostic_index_entries()
        .into_iter()
        .filter(|entry| entry.kind == Some("diagnostic") && !entry.names.is_empty())
        .collect()
}

fn diagnostic_names(doc: &'static str) -> (Option<&'static str>, Vec<&'static str>) {
    for line in doc.lines().map(str::trim) {
        if let Some(names) = line.strip_prefix("Warning name:") {
            return (Some("warning"), backtick_items(names));
        }
        if let Some(names) = line.strip_prefix("Compiler diagnostic name:") {
            return (Some("diagnostic"), backtick_items(names));
        }
    }
    (None, Vec::new())
}

fn diagnostic_name_matches(name: &str, query: &str) -> bool {
    if name.eq_ignore_ascii_case(query) {
        return true;
    }

    let name = name.to_ascii_lowercase();
    let query = query.to_ascii_lowercase();
    name.strip_suffix("<category>")
        .is_some_and(|prefix| query.starts_with(prefix) && query.len() > prefix.len())
}

fn backtick_items(input: &str) -> Vec<&str> {
    input
        .split('`')
        .skip(1)
        .step_by(2)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
struct AttributeDocEntry {
    key: &'static str,
    doc: &'static str,
    names: Vec<String>,
}

fn attribute_doc_entries() -> Vec<AttributeDocEntry> {
    let mut entries: Vec<_> = get_all_attribute_docs()
        .into_iter()
        .map(|(key, doc)| {
            let mut names = attribute_names(key);
            if names.is_empty() {
                names.push(key.to_owned());
            }
            AttributeDocEntry { key, doc, names }
        })
        .collect();
    entries.sort_by(|left, right| left.names[0].cmp(&right.names[0]));
    entries
}

fn attribute_names(key: &str) -> Vec<String> {
    match key {
        "borrow_and_owned" => vec!["borrow".to_owned(), "owned".to_owned()],
        "coverage_skip" => vec!["coverage.skip".to_owned()],
        "doc_hidden" => vec!["doc".to_owned()],
        key => vec![key.to_owned()],
    }
}

fn normalize_attribute_name(name: &str) -> String {
    name.trim()
        .trim_start_matches('#')
        .split('(')
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        attribute_docs, diagnostic_docs, non_warning_diagnostic_index_entries, warning_index,
    };

    #[test]
    fn resolves_prefixed_and_unprefixed_diagnostic_codes() {
        let unprefixed = diagnostic_docs("2");
        assert_eq!(unprefixed.len(), 1);
        assert_eq!(unprefixed, diagnostic_docs("E0002"));
    }

    #[test]
    fn resolves_warning_mnemonics_through_integrated_docs() {
        let docs = diagnostic_docs("unused_value");
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn lists_non_warning_diagnostic_docs() {
        let entries = non_warning_diagnostic_index_entries();
        assert!(
            entries
                .iter()
                .any(|entry| { entry.code == "4056" && entry.names.contains(&"method_duplicate") })
        );
    }

    #[test]
    fn compiler_warning_ids_resolve_through_docs_or_warning_snapshot() {
        for entry in warning_index::warning_entries() {
            let docs = diagnostic_docs(&entry.id.to_string());
            assert_eq!(docs.len(), 1);
            assert!(docs[0].starts_with(&format!("# E{:04}\n", entry.id)));
        }
    }

    #[test]
    fn resolves_compiler_diagnostic_names_through_integrated_docs() {
        let docs = diagnostic_docs("method_duplicate");
        assert_eq!(docs.len(), 1);
        assert!(docs[0].starts_with("# E4056\n"));
    }

    #[test]
    fn resolves_attribute_docs_by_attribute_name() {
        let docs = attribute_docs("coverage.skip");
        assert_eq!(docs.len(), 1);
        assert!(docs[0].starts_with("# Coverage Skip Attribute\n"));
    }

    #[test]
    fn resolves_attribute_docs_by_file_stem() {
        let docs = attribute_docs("borrow_and_owned");
        assert_eq!(docs.len(), 1);
        assert!(docs[0].starts_with("# Borrow and Owned Attribute\n"));
    }
}
