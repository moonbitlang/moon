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

//! The fix driver: decides which of the fixes moonc proposes are applied.
//!
//! moonc attaches machine-applicable `fixes` to JSON diagnostics but only
//! proposes them. Every check happens here, and a fix whose check fails is
//! skipped as a whole, never partly applied:
//!
//! 1. every edit is on one line, inside the line, and its range reads `expect`
//!    after collapsing runs of spaces and tabs (this also catches a file
//!    changed since the check);
//! 2. no two identifier characters become adjacent that were not, checked per
//!    edit and, for edits of one fix that touch, on their combined result,
//!    except when every such edit is `inside_literal`;
//! 3. no `fresh` name occurs in the file or in another selected fix's
//!    replacement text as a whole identifier;
//! 4. the rest of the line after a `followed_by` edit, past blanks, starts
//!    with that text (`""`: is empty);
//! 5. edits of one fix neither overlap nor start at the same point, and a fix
//!    whose edits overlap or start at the same point as those of an already
//!    selected fix is skipped, as is one whose edits touch them with a
//!    combined result that fails check 2;
//! 6. no fix is applied in a file whose positions moonc may count differently:
//!    a line break other than LF or CRLF (lone CR, U+2028, U+2029), or a line
//!    of 65535 code points or more.
//!
//! A fix must also be proposed by every target backend whose check compiled
//! the files it edits; a file that no check compiled is never edited.
//!
//! The checks follow the compiler's reference driver
//! (`test/fix_test/apply_fixes.js`) and contract
//! (`lib/xml/computil/diagnostic_fix.mli`). Selection also protects freshness
//! across fixes and checks touching edits together for joined tokens.

use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// One replacement proposed by moonc, as its JSON contract describes it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
pub(crate) struct Edit {
    pub path: PathBuf,
    /// `L:C-L:C`: 1-based line, 1-based column in code points, end exclusive.
    pub loc: String,
    pub expect: String,
    pub new_text: String,
    #[serde(default)]
    pub fresh: Vec<String>,
    #[serde(default)]
    pub inside_literal: bool,
    #[serde(default)]
    pub followed_by: Option<String>,
}

/// A fix proposed by moonc: edits to apply together or not at all.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
pub(crate) struct Fix {
    pub title: String,
    pub edits: Vec<Edit>,
}

impl Fix {
    /// The preferred fix of a moonc JSON diagnostic: the first of its `fixes`.
    ///
    /// A diagnostic without fixes, or with fixes this driver cannot read,
    /// proposes none.
    pub(crate) fn preferred_of_diagnostic(diagnostic: &serde_json::Value) -> Option<Fix> {
        let first = diagnostic.get("fixes")?.as_array()?.first()?;
        let mut fix = Fix::deserialize(first).ok()?;
        for edit in &mut fix.edits {
            // Match the editable-input keys without following source symlinks.
            edit.path = dunce::simplified(&edit.path).to_owned();
        }
        Some(fix)
    }
}

/// Why a proposed fix was not applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SkipReason {
    /// A target backend that compiled an edited file did not propose it.
    NotProposedByEveryTarget,
    /// An edited file is not an eligible local compiler input.
    NotCompiled,
    /// An edited source file is a symbolic link.
    Symlink,
    /// An edited file could not be read.
    Unreadable,
    /// The fix has no edits.
    NoEdits,
    /// moonc's positions are unreliable in an edited file.
    UnreliablePositions,
    /// An edit location is malformed, spans lines, or lies outside its line.
    BadLocation,
    /// An edit range does not read its `expect` text.
    ExpectMismatch,
    /// The fix would join two identifier characters.
    JoinsTokens,
    /// A `fresh` name already occurs in the file.
    NameNotFresh,
    /// The text after an edit is not its `followed_by` text.
    NotFollowedBy,
    /// Two edits of the fix overlap or start at the same point.
    OverlappingEdits,
    /// An edit overlaps or starts at the same point as an edit of a fix
    /// selected before it, invalidates freshness, or touches one and their
    /// combined result joins tokens.
    ConflictsWithSelectedFix,
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            SkipReason::NotProposedByEveryTarget => {
                "not proposed by every target backend that compiles the file"
            }
            SkipReason::NotCompiled => {
                "the file is not an editable source file compiled by the check"
            }
            SkipReason::Symlink => "the source file is a symlink",
            SkipReason::Unreadable => "the file cannot be read",
            SkipReason::NoEdits => "the fix has no edits",
            SkipReason::UnreliablePositions => {
                "the file has a line break other than LF or CRLF, or a line too long"
            }
            SkipReason::BadLocation => "an edit location is not within one line",
            SkipReason::ExpectMismatch => "the file does not read as the fix expects",
            SkipReason::JoinsTokens => "the fix would join two tokens",
            SkipReason::NameNotFresh => "a name the fix introduces is already used",
            SkipReason::NotFollowedBy => "the text after an edit is not as the fix expects",
            SkipReason::OverlappingEdits => "edits of the fix overlap",
            SkipReason::ConflictsWithSelectedFix => "it conflicts with another fix",
        })
    }
}

/// A fix that was not applied, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkippedFix {
    pub fix: Fix,
    pub reason: SkipReason,
}

/// The new content of one file and the fixes that produce it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileChange {
    pub original: String,
    pub fixed: String,
    /// The applied fixes, in file order.
    pub fixes: Vec<Fix>,
}

/// The outcome of the driver checks: what to write, and what was skipped.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct FixPlan {
    pub changes: BTreeMap<PathBuf, FileChange>,
    pub skipped: Vec<SkippedFix>,
}

impl FixPlan {
    pub(crate) fn applied_fix_count(&self) -> usize {
        self.changes.values().map(|change| change.fixes.len()).sum()
    }
}

/// Selects the fixes to apply and computes the resulting file contents.
///
/// `proposals` holds, for each target backend (any key that tells backends
/// apart), the preferred fix of each of its diagnostics. `compiled` maps each
/// fixable source file to the backends whose check compiled it. A fix is
/// applied only if every backend that compiled a file it edits proposes it,
/// and it passes every check in the module documentation against the file
/// content `read` returns.
pub(crate) fn plan_fixes<K: Ord>(
    proposals: &BTreeMap<K, BTreeSet<Fix>>,
    compiled: &BTreeMap<PathBuf, BTreeSet<K>>,
    mut read: impl FnMut(&Path) -> std::io::Result<String>,
) -> FixPlan {
    let mut plan = FixPlan::default();
    let mut sources = Sources::default();

    let proposed: BTreeSet<&Fix> = proposals.values().flatten().collect();
    let mut resolved = Vec::new();
    for fix in proposed {
        let result =
            agreed(fix, proposals, compiled).and_then(|()| resolve(fix, &mut sources, &mut read));
        match result {
            Ok(edits) => resolved.push((fix, edits)),
            Err(reason) => plan.skipped.push(SkippedFix {
                fix: fix.clone(),
                reason,
            }),
        }
    }

    // Select greedily in file order, like the reference driver.
    resolved.sort_by(|(_, a), (_, b)| (&a[0].path, a[0].start).cmp(&(&b[0].path, b[0].start)));
    let mut taken: BTreeMap<PathBuf, Vec<ResolvedEdit>> = BTreeMap::new();
    let mut selected = Vec::new();
    for (fix, edits) in resolved {
        if conflicts_with_selected(&edits, &taken, &sources) {
            plan.skipped.push(SkippedFix {
                fix: fix.clone(),
                reason: SkipReason::ConflictsWithSelectedFix,
            });
            continue;
        }
        for edit in &edits {
            taken
                .entry(edit.path.clone())
                .or_default()
                .push(edit.clone());
        }
        selected.push((fix, edits));
    }

    // A fix is listed under the file of its first edit; moonc puts all edits
    // of a fix in the file of its diagnostic.
    let mut by_file: BTreeMap<PathBuf, (Vec<Fix>, Vec<ResolvedEdit>)> = BTreeMap::new();
    for (fix, edits) in selected {
        by_file
            .entry(edits[0].path.clone())
            .or_default()
            .0
            .push(fix.clone());
        for edit in edits {
            by_file.entry(edit.path.clone()).or_default().1.push(edit);
        }
    }
    for (path, (fixes, mut edits)) in by_file {
        let original = sources.text(&path).to_owned();
        let mut fixed = original.clone();
        // Right to left, so earlier offsets stay valid.
        edits.sort_by_key(|edit| std::cmp::Reverse(edit.start));
        for edit in edits {
            fixed.replace_range(edit.start..edit.end, &edit.new_text);
        }
        plan.changes.insert(
            path,
            FileChange {
                original,
                fixed,
                fixes,
            },
        );
    }
    plan
}

/// Whether every backend that compiled a file `fix` edits proposes it.
fn agreed<K: Ord>(
    fix: &Fix,
    proposals: &BTreeMap<K, BTreeSet<Fix>>,
    compiled: &BTreeMap<PathBuf, BTreeSet<K>>,
) -> Result<(), SkipReason> {
    for edit in &fix.edits {
        let backends = compiled
            .get(&edit.path)
            .filter(|backends| !backends.is_empty())
            .ok_or(SkipReason::NotCompiled)?;
        let all_propose = backends.iter().all(|backend| {
            proposals
                .get(backend)
                .is_some_and(|fixes| fixes.contains(fix))
        });
        if !all_propose {
            return Err(SkipReason::NotProposedByEveryTarget);
        }
    }
    Ok(())
}

/// File contents read once each, and whether moonc's positions hold in them.
#[derive(Default)]
struct Sources {
    files: BTreeMap<PathBuf, Option<Source>>,
}

struct Source {
    text: String,
    reliable: bool,
}

impl Sources {
    fn get(
        &mut self,
        path: &Path,
        read: &mut impl FnMut(&Path) -> std::io::Result<String>,
    ) -> Result<&Source, SkipReason> {
        let source = match self.files.entry(path.to_owned()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(read(path).ok().map(|text| Source {
                reliable: positions_reliable(&text),
                text,
            })),
        };
        source.as_ref().ok_or(SkipReason::Unreadable)
    }

    /// The text of a file that an earlier `get` read.
    fn text(&self, path: &Path) -> &str {
        &self.files[path]
            .as_ref()
            .expect("only readable files have resolved edits")
            .text
    }
}

/// Whether moonc counts lines and columns of `text` as this driver does.
fn positions_reliable(text: &str) -> bool {
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let lone_cr = c == '\r' && chars.peek() != Some(&'\n');
        if lone_cr || c == '\u{2028}' || c == '\u{2029}' {
            return false;
        }
    }
    text.split('\n').all(|line| line.chars().count() < 65535)
}

/// An edit that passed its checks, as a byte range of its file.
#[derive(Debug, Clone)]
struct ResolvedEdit {
    path: PathBuf,
    start: usize,
    end: usize,
    new_text: String,
    fresh: Vec<String>,
    inside_literal: bool,
}

/// Whether the edits of a fix overlap or start at the same point as edits of
/// fixes selected before it, invalidate each other's fresh names, or touch
/// with a combined result that joins tokens.
fn conflicts_with_selected(
    edits: &[ResolvedEdit],
    selected: &BTreeMap<PathBuf, Vec<ResolvedEdit>>,
    sources: &Sources,
) -> bool {
    for x in edits {
        let Some(taken) = selected.get(&x.path) else {
            continue;
        };
        let overlaps =
            |y: &ResolvedEdit| (x.start < y.end && y.start < x.end) || x.start == y.start;
        if taken.iter().any(overlaps) {
            return true;
        }
        // Freshness is file-wide and must hold in both directions. A later
        // replacement can invalidate a selected fix even without declaring
        // any fresh names of its own. Edits within one fix remain a unit.
        if taken.iter().any(|y| {
            x.fresh
                .iter()
                .any(|name| y.fresh.contains(name) || occurs_as_identifier(&y.new_text, name))
                || y.fresh
                    .iter()
                    .any(|name| occurs_as_identifier(&x.new_text, name))
        }) {
            return true;
        }
        if !taken.iter().any(|y| y.end == x.start || x.end == y.start) {
            continue;
        }
        // The edits around `x` that touch it, directly or through each other,
        // are applied as one replacement: check that replacement.
        let mut run: Vec<&ResolvedEdit> = taken
            .iter()
            .chain(edits.iter().filter(|e| e.path == x.path))
            .collect();
        run.sort_by_key(|e| (e.start, e.end));
        let mut group: Option<(usize, usize, String, bool)> = None;
        for e in run {
            match &mut group {
                Some((_, end, new_text, literal)) if e.start == *end => {
                    *end = e.end;
                    new_text.push_str(&e.new_text);
                    *literal &= e.inside_literal;
                }
                Some((start, end, _, _)) if *start <= x.start && x.end <= *end => break,
                _ => group = Some((e.start, e.end, e.new_text.clone(), e.inside_literal)),
            }
        }
        let (start, end, new_text, literal) = group.expect("`x` is in the run");
        let text = sources.text(&x.path);
        let before = text[..start].chars().next_back();
        let after = text[end..].chars().next();
        let old = &text[start..end];
        if !literal
            && joins_around(
                before,
                old.chars().next(),
                old.chars().next_back(),
                after,
                &new_text,
            )
        {
            return true;
        }
    }
    false
}

/// An edit located in its line, for the checks that look at the line.
struct LineEdit<'a> {
    path: &'a Path,
    line: usize,
    /// The code points of the line, including a CR before its LF.
    chars: Vec<char>,
    /// Code point range in `chars`.
    start: usize,
    end: usize,
    new_text: String,
    inside_literal: bool,
}

/// Checks the edits of `fix` against their files and returns them as byte
/// ranges.
fn resolve(
    fix: &Fix,
    sources: &mut Sources,
    read: &mut impl FnMut(&Path) -> std::io::Result<String>,
) -> Result<Vec<ResolvedEdit>, SkipReason> {
    if fix.edits.is_empty() {
        return Err(SkipReason::NoEdits);
    }
    let mut located = Vec::with_capacity(fix.edits.len());
    let mut resolved = Vec::with_capacity(fix.edits.len());
    for edit in &fix.edits {
        let source = sources.get(&edit.path, read)?;
        if !source.reliable {
            return Err(SkipReason::UnreliablePositions);
        }
        let text = source.text.as_str();
        let (line, c1, line2, c2) = parse_loc(&edit.loc).ok_or(SkipReason::BadLocation)?;
        if line != line2 || line < 1 {
            return Err(SkipReason::BadLocation);
        }
        let (line_start, line_text) = text
            .split('\n')
            .scan(0, |offset, line| {
                let start = *offset;
                *offset += line.len() + 1;
                Some((start, line))
            })
            .nth(line - 1)
            .ok_or(SkipReason::BadLocation)?;
        let chars: Vec<char> = line_text.chars().collect();
        if c1 < 1 || c2 < c1 || c2 > chars.len() + 1 {
            return Err(SkipReason::BadLocation);
        }
        let (start, end) = (c1 - 1, c2 - 1);
        let range_text: String = chars[start..end].iter().collect();
        if collapse_blanks(&range_text) != collapse_blanks(&edit.expect) {
            return Err(SkipReason::ExpectMismatch);
        }
        if !edit.inside_literal && joins_tokens(&chars, start, end, &edit.new_text) {
            return Err(SkipReason::JoinsTokens);
        }
        if let Some(followed_by) = &edit.followed_by {
            let rest: String = chars[end..].iter().collect();
            let rest = rest.trim_start_matches([' ', '\t', '\r']);
            let ok = if followed_by.is_empty() {
                rest.is_empty()
            } else {
                rest.starts_with(followed_by.as_str())
            };
            if !ok {
                return Err(SkipReason::NotFollowedBy);
            }
        }
        if edit
            .fresh
            .iter()
            .any(|name| occurs_as_identifier(text, name))
        {
            return Err(SkipReason::NameNotFresh);
        }
        let byte_offset = |column: usize| {
            line_start + chars[..column].iter().map(|c| c.len_utf8()).sum::<usize>()
        };
        resolved.push(ResolvedEdit {
            path: edit.path.clone(),
            start: byte_offset(start),
            end: byte_offset(end),
            new_text: edit.new_text.clone(),
            fresh: edit.fresh.clone(),
            inside_literal: edit.inside_literal,
        });
        located.push(LineEdit {
            path: &edit.path,
            line,
            chars,
            start,
            end,
            new_text: edit.new_text.clone(),
            inside_literal: edit.inside_literal,
        });
    }
    check_touching_edits(located)?;
    resolved.sort_by(|a, b| (&a.path, a.start).cmp(&(&b.path, b.start)));
    Ok(resolved)
}

/// Edits of one fix that touch are applied together, so check them together:
/// they must not overlap or start at the same point, and their combined
/// result must not join tokens.
fn check_touching_edits(mut edits: Vec<LineEdit<'_>>) -> Result<(), SkipReason> {
    edits.sort_by(|a, b| (a.path, a.line, a.start).cmp(&(b.path, b.line, b.start)));
    let mut edits = edits.into_iter().peekable();
    while let Some(mut group) = edits.next() {
        let mut previous_start = group.start;
        while let Some(edit) = edits.peek()
            && group.path == edit.path
            && group.line == edit.line
            && edit.start <= group.end
        {
            if edit.start < group.end || edit.start == previous_start {
                return Err(SkipReason::OverlappingEdits);
            }
            previous_start = edit.start;
            group.end = edit.end;
            group.new_text.push_str(&edit.new_text);
            group.inside_literal &= edit.inside_literal;
            edits.next();
        }
        // Only the complete replacement determines the final token boundary;
        // a later touching edit can restore a separator removed by a prefix.
        if !group.inside_literal
            && joins_tokens(&group.chars, group.start, group.end, &group.new_text)
        {
            return Err(SkipReason::JoinsTokens);
        }
    }
    Ok(())
}

fn parse_loc(loc: &str) -> Option<(usize, usize, usize, usize)> {
    let number = |s: &str| {
        (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse().ok())
            .flatten()
    };
    let position = |s: &str| {
        let (line, column) = s.split_once(':')?;
        Some((number(line)?, number(column)?))
    };
    let (start, end) = loc.split_once('-')?;
    let (l1, c1) = position(start)?;
    let (l2, c2) = position(end)?;
    Some((l1, c1, l2, c2))
}

fn collapse_blanks(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_blank = false;
    for c in s.chars() {
        if c == ' ' || c == '\t' {
            if !in_blank {
                out.push(' ');
            }
            in_blank = true;
        } else {
            out.push(c);
            in_blank = false;
        }
    }
    out
}

/// An identifier character for the adjacency check: ASCII `[A-Za-z0-9_]` or
/// any non-ASCII character.
fn is_ident(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || !c.is_ascii())
}

/// Whether replacing `chars[start..end]` by `new_text` puts two identifier
/// characters next to each other that were not, e.g. deleting `@pkg.` from
/// `return@pkg.C`.
fn joins_tokens(chars: &[char], start: usize, end: usize, new_text: &str) -> bool {
    let old = &chars[start..end];
    joins_around(
        start.checked_sub(1).map(|i| chars[i]),
        old.first().copied(),
        old.last().copied(),
        chars.get(end).copied(),
        new_text,
    )
}

/// [`joins_tokens`] for a replacement given by the characters around it and
/// the first and last characters of the text it replaces.
fn joins_around(
    before: Option<char>,
    old_first: Option<char>,
    old_last: Option<char>,
    after: Option<char>,
    new_text: &str,
) -> bool {
    let new_first = new_text.chars().next().or(after);
    let new_last = new_text.chars().next_back().or(before);
    let old_first = old_first.or(after);
    let old_last = old_last.or(before);
    let joined = |l: Option<char>, r: Option<char>| is_ident(l) && is_ident(r);
    (joined(before, new_first) && !joined(before, old_first))
        || (joined(new_last, after) && !joined(old_last, after))
}

/// Whether `name` occurs in `text` delimited by non-identifier ASCII
/// characters, the reference driver's notion of a whole identifier.
fn occurs_as_identifier(text: &str, name: &str) -> bool {
    let bytes = text.as_bytes();
    let ident = |b: Option<&u8>| b.is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_');
    let mut from = 0;
    while let Some(offset) = text[from..].find(name) {
        let i = from + offset;
        let before = i.checked_sub(1).and_then(|j| bytes.get(j));
        if !ident(before) && !ident(bytes.get(i + name.len())) {
            return true;
        }
        match text[i..].chars().next() {
            Some(c) => from = i + c.len_utf8(),
            None => return false,
        }
    }
    false
}

#[cfg(test)]
mod tests;
