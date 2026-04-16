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
use moonutil::error_code_docs::get_error_code_doc;
use moonutil::{BINARIES, cli::UniversalFlags};

/// Explain diagnostics from the compiler.
#[derive(Debug, clap::Parser)]
#[clap(
    arg_required_else_help = true,
    after_help = "Resources:\n    Docs: https://docs.moonbitlang.com\n    Skills: https://github.com/moonbitlang/skills\n\n    Use `moon explain --diagnostics` to list warning mnemonics and IDs."
)]
pub(crate) struct ExplainSubcommand {
    /// Explain diagnostics. Without a query, list warning mnemonics and IDs from `moonc`.
    #[clap(
        long,
        num_args = 0..=1,
        default_missing_value = "",
        value_name = "ID_OR_MNEMONIC"
    )]
    pub diagnostics: Option<String>,
}

pub(crate) fn run_explain(_cli: &UniversalFlags, cmd: ExplainSubcommand) -> anyhow::Result<i32> {
    match cmd.diagnostics.as_deref() {
        Some("") => list_diagnostics(),
        Some(query) => explain_diagnostic(query),
        None => unreachable!("clap requires an argument for `moon explain`"),
    }
}

fn list_diagnostics() -> anyhow::Result<i32> {
    let status = Command::new(&*BINARIES.moonc)
        .arg("check")
        .arg("-warn-help")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to run `{}`", BINARIES.moonc.display()))?;

    if status.success() {
        Ok(status.code().unwrap_or(0))
    } else {
        bail!("`moonc check -warn-help` failed")
    }
}

fn explain_diagnostic(query: &str) -> anyhow::Result<i32> {
    let docs = diagnostic_docs(query);
    if docs.is_empty() {
        bail!(
            "no integrated diagnostic docs found for `{}`. Try `moon explain --diagnostics` to list available warnings.",
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

#[cfg(test)]
mod tests {
    use super::diagnostic_docs;

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
}
