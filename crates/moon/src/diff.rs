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

//! Diff rendering shared by interface checks and test failure reports.

use std::io::Write;

use anstyle::{AnsiColor, Style};
use similar::{DiffOp, DiffTag, TextDiff};

const DIFF_CONTEXT_STYLE: Style = AnsiColor::BrightBlack.on_default();
const DIFF_REMOVED_STYLE: Style = AnsiColor::BrightRed.on_default();
const DIFF_REMOVED_EMPHASIS_STYLE: Style = AnsiColor::BrightRed.on_default().underline();
const DIFF_ADDED_STYLE: Style = AnsiColor::BrightGreen.on_default();
const DIFF_ADDED_EMPHASIS_STYLE: Style = AnsiColor::BrightGreen.on_default().underline();
const DIFF_MISSING_NEWLINE_STYLE: Style = Style::new().dimmed();

/// Write the difference between strings `expected` and `actual` to the provided
/// writer `to`, in unified diff format.
///
/// The function uses a number of heuristics to determine the best format for
/// the diff output. Inline diffs will not show newline differences, short diffs
/// will be displayed in full, and long diffs will be chunked.
/// ANSI styles are always rendered; the destination writer decides whether to
/// retain or strip them.
pub(crate) fn write_diff(
    expected: &str,
    actual: &str,
    max_non_chunk_lines: usize,
    diff_ctx: usize,
    mut to: impl Write,
) -> std::io::Result<()> {
    // Determine if we want to chunk the diff output
    let expected_lines = expected.lines().count();
    let actual_lines = actual.lines().count();
    let use_chunking = expected_lines > max_non_chunk_lines || actual_lines > max_non_chunk_lines;
    let inline = expected_lines <= 1 && actual_lines <= 1;

    let diff = similar::TextDiff::configure()
        .algorithm(similar::Algorithm::Patience)
        .diff_lines(expected, actual);

    if inline {
        write_hunk(&diff, diff.ops(), &mut to, false, false)?;
    } else if use_chunking {
        let grouped = diff.grouped_ops(diff_ctx);
        for hunk in grouped {
            write_hunk(&diff, &hunk, &mut to, true, true)?;
        }
    } else {
        write_hunk(&diff, diff.ops(), &mut to, false, true)?;
    }

    Ok(())
}

#[test]
fn diff_color_follows_destination_writer() {
    let render = |choice| {
        let mut writer = anstream::AutoStream::new(Vec::new(), choice);
        write_diff("old\n", "new\n", 1, 2, &mut writer).unwrap();
        writer.into_inner()
    };

    let colored = render(anstream::ColorChoice::AlwaysAnsi);
    assert!(
        colored.contains(&b'\x1b'),
        "output was not colored: {colored:?}"
    );

    let plain = render(anstream::ColorChoice::Never);
    assert!(!plain.contains(&b'\x1b'));
    assert!(
        plain.starts_with(b"-old\n+new\n"),
        "unexpected diff: {plain:?}"
    );
}

/// Write a hunk of diff
fn write_hunk<'a>(
    orig_diff: &'a TextDiff<'a, 'a, 'a, str>,
    hunk: &[DiffOp],
    mut to: impl Write,
    header: bool,
    report_missing_nl: bool,
) -> std::io::Result<()> {
    if header {
        let header = similar::udiff::UnifiedHunkHeader::new(hunk);
        writeln!(to, "{DIFF_CONTEXT_STYLE}{header}{DIFF_CONTEXT_STYLE:#}")?;
    }

    for op in hunk {
        if op.tag() == DiffTag::Equal {
            // Print plain lines
            for line in orig_diff.iter_changes(op) {
                write!(to, " {}", line.value())?;
                report_missing_nl_f(&mut to, report_missing_nl, line.missing_newline())?;
            }
        } else {
            for change in orig_diff.iter_inline_changes(op) {
                match change.tag() {
                    similar::ChangeTag::Equal => {
                        unreachable!("Equal should have been handled earlier")
                    }
                    similar::ChangeTag::Delete => {
                        write!(to, "{DIFF_REMOVED_STYLE}-{DIFF_REMOVED_STYLE:#}")?;
                        for &(emph, slice) in change.values() {
                            if emph {
                                write!(
                                    to,
                                    "{DIFF_REMOVED_EMPHASIS_STYLE}{slice}{DIFF_REMOVED_EMPHASIS_STYLE:#}"
                                )?;
                            } else {
                                write!(to, "{DIFF_REMOVED_STYLE}{slice}{DIFF_REMOVED_STYLE:#}")?;
                            }
                        }
                    }
                    similar::ChangeTag::Insert => {
                        write!(to, "{DIFF_ADDED_STYLE}+{DIFF_ADDED_STYLE:#}")?;
                        for &(emph, slice) in change.values() {
                            if emph {
                                write!(
                                    to,
                                    "{DIFF_ADDED_EMPHASIS_STYLE}{slice}{DIFF_ADDED_EMPHASIS_STYLE:#}"
                                )?;
                            } else {
                                write!(to, "{DIFF_ADDED_STYLE}{slice}{DIFF_ADDED_STYLE:#}")?;
                            }
                        }
                    }
                }

                report_missing_nl_f(&mut to, report_missing_nl, change.missing_newline())?;
            }
        }
    }
    Ok(())
}

fn report_missing_nl_f(
    mut to: impl Write,
    report_missing_newlines: bool,
    missing: bool,
) -> Result<(), std::io::Error> {
    if missing {
        writeln!(to)?;
        if report_missing_newlines {
            writeln!(
                to,
                "{DIFF_MISSING_NEWLINE_STYLE}\\ No newline at end of file{DIFF_MISSING_NEWLINE_STYLE:#}"
            )?;
        }
    }
    Ok(())
}
