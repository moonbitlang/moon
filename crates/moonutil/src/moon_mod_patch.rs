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

//! Text patching support for `moon.mod`.
//!
//! A serde-based patch would drop comments because the `moon.mod` parser lowers the file to a
//! JSON-like structure. That structure is intentionally kept compatible with the old
//! `moon.mod.json` model, so it does not preserve source locations either.
//!
//! After discussion with @bobzhang, we patch `moon.mod` text directly in `moon` instead of
//! round-tripping through an AST and `moonfmt`. This keeps comments intact and avoids depending on
//! `moonfmt` for every small manifest edit.

use std::{ops::Range, path::Path};

use anyhow::Context;
use indexmap::IndexMap;

use crate::{
    common::{MOON_MOD, write_module_dsl_to_file},
    module::MoonModJSON,
    moon_pkg,
};

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum MoonModPatch {
    UpsertImportItem {
        name: String,
        version: semver::Version,
    },
    RemoveImportItem {
        name: String,
    },
    UpdateImportItems(IndexMap<String, semver::Version>),
    Rewrite(MoonModJSON),
}

#[derive(Debug)]
struct ImportItemRange {
    string: Range<usize>,
    trailing: Range<usize>,
}

struct MoonModImportBlock {
    open: usize,
    close: usize,
    tokens: Vec<moon_pkg::Token>,
}

fn byte_offset_for_pos(contents: &str, pos: &moon_pkg::Pos) -> usize {
    let mut line = 1usize;
    let mut offset = 0usize;
    for segment in contents.split_inclusive('\n') {
        if line == pos.line {
            return offset + pos.column.saturating_sub(1);
        }
        offset += segment.len();
        line += 1;
    }
    contents.len()
}

fn token_byte_range(contents: &str, token: &moon_pkg::Token) -> Range<usize> {
    let loc = token.range();
    byte_offset_for_pos(contents, &loc.start)..byte_offset_for_pos(contents, &loc.end)
}

fn locate_import_block(contents: &str) -> Option<MoonModImportBlock> {
    let Ok(tokens) = moon_pkg::tokenize(contents) else {
        return None;
    };
    let mut depth = 0usize;
    let mut index = 0usize;
    while index < tokens.len() {
        let token = &tokens[index];
        if depth == 0 && matches!(token, moon_pkg::Token::IMPORT(_)) {
            let open_index = index.checked_add(1)?;
            if !matches!(tokens.get(open_index), Some(moon_pkg::Token::LBRACE(_))) {
                return None;
            }

            let mut import_depth = 0usize;
            let mut close_index = None;
            for (index, token) in tokens.iter().enumerate().skip(open_index) {
                match token {
                    moon_pkg::Token::LBRACE(_) => import_depth += 1,
                    moon_pkg::Token::RBRACE(_) => {
                        import_depth = import_depth.saturating_sub(1);
                        if import_depth == 0 {
                            close_index = Some(index);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let close_index = close_index?;

            return Some(MoonModImportBlock {
                open: open_index,
                close: close_index,
                tokens,
            });
        }

        match token {
            moon_pkg::Token::LBRACE(_)
            | moon_pkg::Token::LPAREN(_)
            | moon_pkg::Token::LBRACKET(_) => depth += 1,
            moon_pkg::Token::RBRACE(_)
            | moon_pkg::Token::RPAREN(_)
            | moon_pkg::Token::RBRACKET(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
        index += 1;
    }
    None
}

fn locate_import_rbrace(contents: &str) -> Option<Range<usize>> {
    let import = locate_import_block(contents)?;
    let close = token_byte_range(contents, &import.tokens[import.close]);
    Some(close.start..close.start)
}

fn import_item_trailing_range(
    contents: &str,
    string: Range<usize>,
    next: Option<&moon_pkg::Token>,
) -> Range<usize> {
    let Some(next) = next.filter(|token| matches!(token, moon_pkg::Token::COMMA(_))) else {
        return string.end..string.end;
    };
    let comma = token_byte_range(contents, next);
    let mut end = string.end;
    for (offset, ch) in contents[string.end..comma.start].char_indices() {
        if contents[string.end + offset..].starts_with("//") {
            return string.end..end;
        }
        if !ch.is_whitespace() {
            return string.end..end;
        }
        end = string.end + offset + ch.len_utf8();
    }

    end = comma.end;
    for (offset, ch) in contents[comma.end..].char_indices() {
        if contents[comma.end + offset..].starts_with("//") {
            break;
        }
        if !ch.is_whitespace() {
            break;
        }
        end = comma.end + offset + ch.len_utf8();
    }
    string.end..end
}

fn locate_import_items(contents: &str, names: Vec<String>) -> IndexMap<String, ImportItemRange> {
    let Some(import) = locate_import_block(contents) else {
        return IndexMap::new();
    };

    let mut items = IndexMap::new();
    for (index, token) in import
        .tokens
        .iter()
        .enumerate()
        .take(import.close)
        .skip(import.open + 1)
    {
        if let moon_pkg::Token::STRING((_, spec)) = token
            && spec.rsplit_once('@').is_some_and(|(dep_name, version)| {
                names.iter().any(|name| name == dep_name) && !version.is_empty()
            })
            && let Some((dep_name, _)) = spec.rsplit_once('@')
        {
            let string = token_byte_range(contents, token);
            let trailing =
                import_item_trailing_range(contents, string.clone(), import.tokens.get(index + 1));
            items.insert(dep_name.to_string(), ImportItemRange { string, trailing });
        }
    }
    items
}

pub fn patch_module_dsl(mut patched: String, patch: MoonModPatch) -> String {
    match patch {
        MoonModPatch::UpsertImportItem { name, version } => {
            let quoted = format!("\"{name}@{version}\"");
            let mut items = locate_import_items(&patched, vec![name.clone()]);
            if let Some(item) = items.shift_remove(&name) {
                patched.replace_range(item.string, &quoted);
            } else if let Some(tail) = locate_import_rbrace(&patched) {
                patched.insert_str(tail.start, &format!("  {quoted},\n"));
            } else {
                patched = format!("{patched}\nimport {{\n  {quoted},\n}}\n");
            }
        }
        MoonModPatch::RemoveImportItem { name } => {
            let mut items = locate_import_items(&patched, vec![name.clone()]);
            if let Some(item) = items.shift_remove(&name) {
                patched.replace_range(item.trailing, "");
                patched.replace_range(item.string, "");
            }
        }
        MoonModPatch::UpdateImportItems(versions) => {
            let names = versions.keys().cloned().collect::<Vec<_>>();
            let items = locate_import_items(&patched, names);
            let mut replacements = Vec::new();
            for (name, version) in versions {
                if let Some(item) = items.get(&name) {
                    replacements.push((item.string.clone(), format!("\"{name}@{version}\"")));
                }
            }
            replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
            for (range, quoted) in replacements {
                patched.replace_range(range, &quoted);
            }
        }
        MoonModPatch::Rewrite(_) => {
            unreachable!("rewrite patch must be handled by patch_module_dsl_to_file")
        }
    }
    patched
}

pub fn patch_module_dsl_to_file(source_dir: &Path, patch: MoonModPatch) -> anyhow::Result<()> {
    if let MoonModPatch::Rewrite(module) = patch {
        return write_module_dsl_to_file(&module, source_dir);
    }

    let path = source_dir.join(MOON_MOD);
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to load `{}`", path.display()))?;
    let patched = patch_module_dsl(contents, patch);
    std::fs::write(&path, patched)
        .with_context(|| format!("failed to write `{}`", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use expect_test::expect;
    use semver::Version;

    use super::*;

    #[test]
    fn patch_module_dsl_inserts_import() {
        let input = r#"// module comment
name = "example/mod"

// options comment
options(
  source: "src",
)
"#
        .to_string();
        let patch = MoonModPatch::UpsertImportItem {
            name: "example/dep".to_string(),
            version: Version::new(1, 2, 3),
        };
        let output = patch_module_dsl(input, patch);

        expect![[r#"
            // module comment
            name = "example/mod"

            // options comment
            options(
              source: "src",
            )

            import {
              "example/dep@1.2.3",
            }
        "#]]
        .assert_eq(&output);
    }

    #[test]
    fn patch_module_dsl_inserts_into_existing_import() {
        let input = r#"name = "example/mod"

import {
  // dependency group
  "example/other@0.1.0",
  // comment
}
"#
        .to_string();
        let patch = MoonModPatch::UpsertImportItem {
            name: "example/dep".to_string(),
            version: Version::new(1, 2, 3),
        };
        let output = patch_module_dsl(input, patch);

        expect![[r#"
            name = "example/mod"

            import {
              // dependency group
              "example/other@0.1.0",
              // comment
              "example/dep@1.2.3",
            }
        "#]]
        .assert_eq(&output);
    }

    #[test]
    fn patch_module_dsl_upserts_existing_import() {
        let input = r#"name = "example/mod"

import {
  "example/dep@1.0.0", // dep comment
  "example/other@0.1.0",
}
"#
        .to_string();
        let patch = MoonModPatch::UpsertImportItem {
            name: "example/dep".to_string(),
            version: Version::new(1, 2, 3),
        };
        let output = patch_module_dsl(input, patch);

        expect![[r#"
            name = "example/mod"

            import {
              "example/dep@1.2.3", // dep comment
              "example/other@0.1.0",
            }
        "#]]
        .assert_eq(&output);
    }

    #[test]
    fn patch_module_dsl_syncs_versions() {
        let input = r#"name = "example/mod"

import {
  "example/dep@1.0.0", // dep comment1
  // dep comment2
  "example/other@0.1.0",
}
"#
        .to_string();
        let patch = MoonModPatch::UpdateImportItems(IndexMap::from([
            ("example/dep".to_string(), Version::new(1, 2, 3)),
            ("example/other".to_string(), Version::new(0, 2, 0)),
        ]));
        let output = patch_module_dsl(input, patch);

        expect![[r#"
            name = "example/mod"

            import {
              "example/dep@1.2.3", // dep comment1
              // dep comment2
              "example/other@0.2.0",
            }
        "#]]
        .assert_eq(&output);
    }

    #[test]
    fn patch_module_dsl_removes_import_item_preserving_comments() {
        let input = r#"name = "example/mod"

import {
  // dependency group
  "example/dep@1.0.0", // dep comment1
  // dep comment2
  "example/other@0.1.0",
  // dep comment3
}
"#
        .to_string();
        let patch = MoonModPatch::RemoveImportItem {
            name: "example/dep".to_string(),
        };
        let output = patch_module_dsl(input, patch);

        expect![[r#"
            name = "example/mod"

            import {
              // dependency group
              // dep comment1
              // dep comment2
              "example/other@0.1.0",
              // dep comment3
            }
        "#]]
        .assert_eq(&output);
    }

    #[test]
    fn patch_module_dsl_remove_without_import_is_noop() {
        let input = r#"name = "example/mod"

options(
  source: "src",
)
"#
        .to_string();
        let patch = MoonModPatch::RemoveImportItem {
            name: "example/dep".to_string(),
        };
        let output = patch_module_dsl(input, patch);

        expect![[r#"
            name = "example/mod"

            options(
              source: "src",
            )
        "#]]
        .assert_eq(&output);
    }

    #[test]
    fn patch_module_dsl_sync_versions_outside_import_is_noop() {
        let input = r#"name = "example/mod"

options(
  deps: {
    "example/dep": "1.0.0",
  },
)
"#
        .to_string();
        let patch = MoonModPatch::UpdateImportItems(IndexMap::from([(
            "example/dep".to_string(),
            Version::new(1, 2, 3),
        )]));
        let output = patch_module_dsl(input, patch);

        expect![[r#"
            name = "example/mod"

            options(
              deps: {
                "example/dep": "1.0.0",
              },
            )
        "#]]
        .assert_eq(&output);
    }

    #[test]
    fn locate_import_items_snapshot() {
        let input = r#"name = "example/mod"

import {
  // dependency group
  "example/dep@1.0.0", // dep comment
  "example/other@0.1.0",
}
"#;
        let names = vec!["example/dep".to_string(), "example/other".to_string()];
        let output = locate_import_items(input, names);

        expect![[r#"
            {
                "example/dep": ImportItemRange {
                    string: 55..74,
                    trailing: 74..76,
                },
                "example/other": ImportItemRange {
                    string: 93..114,
                    trailing: 114..116,
                },
            }
        "#]]
        .assert_debug_eq(&output);
    }

    #[test]
    fn locate_import_rbrace_snapshot() {
        let input = r#"name = "example/mod"

import {
  // dependency group
  "example/dep@1.0.0", // dep comment
  "example/other@0.1.0",
}
"#;
        let output = locate_import_rbrace(input);

        expect![[r#"
            Some(
                116..116,
            )
        "#]]
        .assert_debug_eq(&output);
    }

    // Expected moon.mod input is moonfmt-formatted with import items split across lines.
    // These single-line cases are here to keep patching correct on edge-case input; the
    // output format for such input is not expected to be polished.
    #[test]
    fn patch_module_dsl_inserts_into_single_line_import() {
        let input = r#"name = "example/mod"

import { "example/other@0.1.0", }
"#
        .to_string();
        let patch = MoonModPatch::UpsertImportItem {
            name: "example/dep".to_string(),
            version: Version::new(1, 2, 3),
        };
        let output = patch_module_dsl(input, patch);

        expect![[r#"
            name = "example/mod"

            import { "example/other@0.1.0",   "example/dep@1.2.3",
            }
        "#]]
        .assert_eq(&output);
    }

    #[test]
    fn patch_module_dsl_syncs_single_line_import_versions() {
        let input = r#"name = "example/mod"

import { "example/dep@1.0.0", "example/other@0.1.0", }
"#
        .to_string();
        let patch = MoonModPatch::UpdateImportItems(IndexMap::from([
            ("example/dep".to_string(), Version::new(1, 2, 3)),
            ("example/other".to_string(), Version::new(0, 2, 0)),
        ]));
        let output = patch_module_dsl(input, patch);

        expect![[r#"
            name = "example/mod"

            import { "example/dep@1.2.3", "example/other@0.2.0", }
        "#]]
        .assert_eq(&output);
    }

    #[test]
    fn patch_module_dsl_removes_from_single_line_import() {
        let input = r#"name = "example/mod"

import { "example/dep@1.0.0", "example/other@0.1.0", }
"#
        .to_string();
        let patch = MoonModPatch::RemoveImportItem {
            name: "example/dep".to_string(),
        };
        let output = patch_module_dsl(input, patch);

        expect![[r#"
            name = "example/mod"

            import { "example/other@0.1.0", }
        "#]]
        .assert_eq(&output);
    }

    #[test]
    fn locate_import_items_single_line_snapshot() {
        let input = r#"name = "example/mod"

import { "example/dep@1.0.0", "example/other@0.1.0", }
"#;
        let names = vec!["example/dep".to_string(), "example/other".to_string()];
        let output = locate_import_items(input, names);

        expect![[r#"
            {
                "example/dep": ImportItemRange {
                    string: 31..50,
                    trailing: 50..52,
                },
                "example/other": ImportItemRange {
                    string: 52..73,
                    trailing: 73..75,
                },
            }
        "#]]
        .assert_debug_eq(&output);
    }
}
