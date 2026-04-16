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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WarningEntry {
    pub(crate) mnemonic: &'static str,
    pub(crate) description: &'static str,
    pub(crate) id: u16,
}

impl WarningEntry {
    pub(crate) fn render_markdown(&self) -> String {
        format!(
            r#"# E{id:04}

Warning name: `{mnemonic}`

{description}"#,
            id = self.id,
            mnemonic = self.mnemonic,
            description = self.description,
        )
    }
}

// Snapshot generated from `moonc check -warn-help` on 2026-04-15.
// Update this file manually when the compiler's warning table changes.
const WARNING_ENTRIES: &[WarningEntry] = &[
    WarningEntry {
        mnemonic: "unused_value",
        description: "Unused variable or function.",
        id: 1,
    },
    WarningEntry {
        mnemonic: "unused_value",
        description: "Unused variable.",
        id: 2,
    },
    WarningEntry {
        mnemonic: "unused_type_declaration",
        description: "Unused type declaration.",
        id: 3,
    },
    WarningEntry {
        mnemonic: "missing_priv",
        description: "Unused abstract type.",
        id: 4,
    },
    WarningEntry {
        mnemonic: "unused_type_variable",
        description: "Unused type variable.",
        id: 5,
    },
    WarningEntry {
        mnemonic: "unused_constructor",
        description: "Unused constructor.",
        id: 6,
    },
    WarningEntry {
        mnemonic: "unused_field",
        description: "Unused field or constructor argument.",
        id: 7,
    },
    WarningEntry {
        mnemonic: "redundant_modifier",
        description: "Redundant modifier.",
        id: 8,
    },
    WarningEntry {
        mnemonic: "struct_never_constructed",
        description: "Struct never constructed.",
        id: 9,
    },
    WarningEntry {
        mnemonic: "unused_pattern",
        description: "Unused pattern.",
        id: 10,
    },
    WarningEntry {
        mnemonic: "partial_match",
        description: "Partial pattern matching.",
        id: 11,
    },
    WarningEntry {
        mnemonic: "unreachable_code",
        description: "Unreachable code.",
        id: 12,
    },
    WarningEntry {
        mnemonic: "unresolved_type_variable",
        description: "Unresolved type variable.",
        id: 13,
    },
    WarningEntry {
        mnemonic: "alert or alert_<category>",
        description: "All alerts or alerts with specific category.",
        id: 14,
    },
    WarningEntry {
        mnemonic: "unused_mut",
        description: "Unused mutability.",
        id: 15,
    },
    WarningEntry {
        mnemonic: "parser_inconsistency",
        description: "Parser inconsistency check.",
        id: 16,
    },
    WarningEntry {
        mnemonic: "ambiguous_loop_argument",
        description: "Ambiguous usage of loop argument.",
        id: 17,
    },
    WarningEntry {
        mnemonic: "useless_loop",
        description: "Useless loop expression.",
        id: 18,
    },
    WarningEntry {
        mnemonic: "deprecated",
        description: "Deprecated API usage.",
        id: 20,
    },
    WarningEntry {
        mnemonic: "missing_pattern_arguments",
        description: "Some arguments of constructor are omitted in pattern.",
        id: 21,
    },
    WarningEntry {
        mnemonic: "ambiguous_block",
        description: "Ambiguous block.",
        id: 22,
    },
    WarningEntry {
        mnemonic: "unused_try",
        description: "Useless try expression.",
        id: 23,
    },
    WarningEntry {
        mnemonic: "unused_error_type",
        description: "Useless error type.",
        id: 24,
    },
    WarningEntry {
        mnemonic: "test_unqualified_package",
        description: "Using implicitly imported API in test.",
        id: 25,
    },
    WarningEntry {
        mnemonic: "unused_catch_all",
        description: "Useless catch all.",
        id: 26,
    },
    WarningEntry {
        mnemonic: "deprecated_syntax",
        description: "Deprecated syntax.",
        id: 27,
    },
    WarningEntry {
        mnemonic: "todo",
        description: "Todo",
        id: 28,
    },
    WarningEntry {
        mnemonic: "unused_package",
        description: "Unused package.",
        id: 29,
    },
    WarningEntry {
        mnemonic: "missing_package_alias",
        description: "Empty package alias.",
        id: 30,
    },
    WarningEntry {
        mnemonic: "unused_optional_argument",
        description: "Optional argument never supplied.",
        id: 31,
    },
    WarningEntry {
        mnemonic: "unused_default_value",
        description: "Default value of optional argument never used.",
        id: 32,
    },
    WarningEntry {
        mnemonic: "text_segment_excceed",
        description: "Text segment exceed the line or column limits.",
        id: 33,
    },
    WarningEntry {
        mnemonic: "implicit_use_builtin",
        description: "Implicit use of definitions from `moonbitlang/core/builtin`.",
        id: 34,
    },
    WarningEntry {
        mnemonic: "reserved_keyword",
        description: "Reserved keyword.",
        id: 35,
    },
    WarningEntry {
        mnemonic: "loop_label_shadowing",
        description: "Loop label shadows another label.",
        id: 36,
    },
    WarningEntry {
        mnemonic: "unused_loop_label",
        description: "Unused loop label.",
        id: 37,
    },
    WarningEntry {
        mnemonic: "missing_invariant",
        description: "For-loop is missing an invariant.",
        id: 38,
    },
    WarningEntry {
        mnemonic: "missing_reasoning",
        description: "For-loop is missing a proof_reasoning.",
        id: 39,
    },
    WarningEntry {
        mnemonic: "multiline_string_escape",
        description: "Deprecated escape sequence in multiline string.",
        id: 40,
    },
    WarningEntry {
        mnemonic: "missing_rest_mark",
        description: "Missing `..` in map pattern.",
        id: 41,
    },
    WarningEntry {
        mnemonic: "invalid_attribute",
        description: "Invalid attribute.",
        id: 42,
    },
    WarningEntry {
        mnemonic: "unused_attribute",
        description: "Unused attribute.",
        id: 43,
    },
    WarningEntry {
        mnemonic: "invalid_inline_wasm",
        description: "Invalid inline-wasm.",
        id: 44,
    },
    WarningEntry {
        mnemonic: "unused_rest_mark",
        description: "Useless `..` in pattern",
        id: 46,
    },
    WarningEntry {
        mnemonic: "invalid_mbti",
        description: "Invalid mbti file",
        id: 47,
    },
    WarningEntry {
        mnemonic: "missing_definition",
        description: "Unused pub definition because it does not exist in mbti file.",
        id: 49,
    },
    WarningEntry {
        mnemonic: "method_shadowing",
        description: "Local method shadows upstream method",
        id: 50,
    },
    WarningEntry {
        mnemonic: "ambiguous_precedence",
        description: "Ambiguous operator precedence",
        id: 51,
    },
    WarningEntry {
        mnemonic: "unused_loop_variable",
        description: "Loop variable not updated in loop",
        id: 52,
    },
    WarningEntry {
        mnemonic: "unused_trait_bound",
        description: "Unused trait bound",
        id: 53,
    },
    WarningEntry {
        mnemonic: "ambiguous_range_direction",
        description: "Ambiguous looping direction for range e1..=e2",
        id: 54,
    },
    WarningEntry {
        mnemonic: "unannotated_ffi",
        description: "Unannotated FFI param type",
        id: 55,
    },
    WarningEntry {
        mnemonic: "missing_pattern_field",
        description: "Missing field in struct pattern",
        id: 56,
    },
    WarningEntry {
        mnemonic: "missing_pattern_payload",
        description: "Constructor pattern expect payload",
        id: 57,
    },
    WarningEntry {
        mnemonic: "unaligned_byte_access",
        description: "Unaligned byte access in bits pattern",
        id: 59,
    },
    WarningEntry {
        mnemonic: "unused_struct_update",
        description: "Unused struct update",
        id: 60,
    },
    WarningEntry {
        mnemonic: "duplicate_test",
        description: "Duplicate test name",
        id: 61,
    },
    WarningEntry {
        mnemonic: "invalid_cascade",
        description: "Calling method with non-unit return type via `..`",
        id: 62,
    },
    WarningEntry {
        mnemonic: "syntax_lint",
        description: "Syntax lint warning",
        id: 63,
    },
    WarningEntry {
        mnemonic: "unannotated_toplevel_array",
        description: "Unannotated toplevel array",
        id: 64,
    },
    WarningEntry {
        mnemonic: "prefer_readonly_array",
        description: "Suggest ReadOnlyArray for read-only array literal",
        id: 65,
    },
    WarningEntry {
        mnemonic: "prefer_fixed_array",
        description: "Suggest FixedArray for mutated array literal",
        id: 66,
    },
    WarningEntry {
        mnemonic: "unused_async",
        description: "Useless `async` annotation",
        id: 67,
    },
    WarningEntry {
        mnemonic: "declaration_unimplemented",
        description: "Declaration is unimplemented",
        id: 68,
    },
    WarningEntry {
        mnemonic: "declaration_implemented",
        description: "Declaration is already implemented",
        id: 69,
    },
    WarningEntry {
        mnemonic: "deprecated_for_in_method",
        description: "using `iterator()` method for `for .. in` loop.",
        id: 70,
    },
    WarningEntry {
        mnemonic: "core_package_not_imported",
        description: "Packages in `moonbitlang/core` need to be explicitly imported.",
        id: 71,
    },
    WarningEntry {
        mnemonic: "unqualified_local_using",
        description: "unqualified local using",
        id: 72,
    },
    WarningEntry {
        mnemonic: "unnecessary_annotation",
        description: "unnecessary type annotation",
        id: 73,
    },
    WarningEntry {
        mnemonic: "missing_doc",
        description: "Missing documentation for public definition",
        id: 74,
    },
];

pub(crate) fn get_warning_entry(id: u16) -> Option<&'static WarningEntry> {
    WARNING_ENTRIES.iter().find(|entry| entry.id == id)
}

pub(crate) fn get_warning_entries_by_mnemonic(mnemonic: &str) -> Vec<&'static WarningEntry> {
    WARNING_ENTRIES
        .iter()
        .filter(|entry| entry.mnemonic.eq_ignore_ascii_case(mnemonic))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use anyhow::{Context, bail};
    use expect_test::expect_file;
    use moonutil::BINARIES;

    use super::{WARNING_ENTRIES, get_warning_entries_by_mnemonic, get_warning_entry};

    fn warning_index_snapshot() -> String {
        WARNING_ENTRIES
            .iter()
            .map(|entry| format!("{:04}\t{}\t{}", entry.id, entry.mnemonic, entry.description))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn normalize_warn_help(output: &str) -> anyhow::Result<String> {
        let header = output
            .lines()
            .find(|line| line.starts_with("mnemonic"))
            .context("missing `moonc check -warn-help` table header")?;
        let description_start = header
            .find("description")
            .context("missing description column in `moonc check -warn-help`")?;
        let id_start = header
            .find(" id ")
            .map(|index| index + 1)
            .context("missing id column in `moonc check -warn-help`")?;
        let state_start = header
            .find("state")
            .context("missing state column in `moonc check -warn-help`")?;

        let entries = output
            .lines()
            .skip_while(|line| !line.starts_with("mnemonic"))
            .skip(1)
            .filter_map(|line| {
                let id = line
                    .get(id_start..state_start)
                    .unwrap_or("")
                    .trim()
                    .parse::<u16>()
                    .ok()?;
                Some(format!(
                    "{id:04}\t{}\t{}",
                    line.get(..description_start).unwrap_or("").trim_end(),
                    line.get(description_start..id_start).unwrap_or("").trim()
                ))
            })
            .collect::<Vec<_>>();

        if entries.is_empty() {
            bail!("found no warning entries in `moonc check -warn-help` output");
        }

        Ok(entries.join("\n"))
    }

    #[test]
    fn preserves_duplicate_mnemonics() {
        let entries = get_warning_entries_by_mnemonic("unused_value");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, 1);
        assert_eq!(entries[1].id, 2);
    }

    #[test]
    fn keeps_long_mnemonics_split_correctly() {
        let entry = get_warning_entry(64).unwrap();
        assert_eq!(entry.mnemonic, "unannotated_toplevel_array");
        assert_eq!(entry.description, "Unannotated toplevel array");
    }

    #[test]
    fn warning_index_snapshot_matches_checked_in_entries() {
        expect_file!["snapshots/explain_warning_index.txt"].assert_eq(&warning_index_snapshot());
    }

    #[test]
    fn warning_index_snapshot_matches_moonc_warn_help() -> anyhow::Result<()> {
        let output = Command::new(&*BINARIES.moonc)
            .arg("check")
            .arg("-warn-help")
            .output()
            .with_context(|| format!("failed to run `{}`", BINARIES.moonc.display()))?;

        if !output.status.success() {
            bail!(
                "`moonc check -warn-help` failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        expect_file!["snapshots/explain_warning_index.txt"]
            .assert_eq(&normalize_warn_help(std::str::from_utf8(&output.stdout)?)?);

        Ok(())
    }
}
