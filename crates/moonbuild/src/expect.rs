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

use anyhow::Context;
use base64::Engine;
use colored::Colorize;
use core::fmt;
use moonutil::common::line_col_to_byte_idx;
use moonutil::module::ModuleDB;
use similar::DiffOp;
use similar::DiffTag;
use similar::TextDiff;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;

pub trait PackageSrcResolver {
    fn resolve_pkg_src(&self, pkg_path: &str) -> PathBuf;
}

impl PackageSrcResolver for ModuleDB {
    fn resolve_pkg_src(&self, pkg_path: &str) -> PathBuf {
        self.get_package_by_name(pkg_path).root_path.clone()
    }
}

#[derive(Debug, Default)]
pub struct PackagePatch {
    patches: HashMap<String, Vec<BufferExpect>>,
}

impl PackagePatch {
    pub fn add(&mut self, filename: &str, patch: BufferExpect) {
        match self.patches.get_mut(filename) {
            Some(vec) => vec.push(patch),
            None => {
                self.patches.insert(filename.to_string(), vec![patch]);
            }
        }
    }
}

#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct BufferExpect {
    range: line_index::TextRange,
    // only for debug
    line_start: i32,
    col_start: i32,
    line_end: i32,
    col_end: i32,
    // end
    left_padding: Option<String>,
    right_padding: Option<String>,
    // Indicates whether the expect content starts with a left parenthesis `(`
    // immediately following `content=` in the inspect function call.
    #[allow(unused)]
    expect: String,
    actual: String,
    kind: TargetKind,
    mode: Option<String>,
    is_doc_test: bool,
}

// something like array out of bounds, moonbit panic & abort catch by js
pub const ERROR: &str = "Error";
// something like division by zero, overflow, etc.
pub const RUNTIME_ERROR: &str = "RuntimeError";
// control by moonbit
pub const FAILED: &str = "FAILED";
// control by moonbit
pub const EXPECT_FAILED: &str = "@EXPECT_FAILED ";
pub const SNAPSHOT_TESTING: &str = "@SNAPSHOT_TESTING ";

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum TargetKind {
    #[default]
    Trivial,
    Pipe,
    Call,
}

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Target {
    left_border_line_start: u32,
    left_border_col_start: u32,

    line_start: u32,
    col_start: u32,
    line_end: u32,
    col_end: u32,

    kind: TargetKind,
    expect: String,
    actual: String,
    mode: Option<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LocationJson {
    pkg: String,
    filename: String,
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ExpectFailedRaw {
    pub loc: LocationJson,
    pub args_loc: Vec<Option<LocationJson>>,
    pub expect: Option<String>,
    pub actual: Option<String>,
    pub expect_base64: Option<String>,
    pub actual_base64: Option<String>,
    pub snapshot: Option<bool>,
    pub mode: Option<String>,
}

impl std::str::FromStr for ExpectFailedRaw {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        // FIXME: dude, WTF is happening here?
        let expect_index = s
            .find("\"expect\":")
            .context(format!("expect field not found: {s}"))?;
        let expect_base64_index = s.find("\"expect_base64\":");
        (if let Some(expect_base64_index) = expect_base64_index {
            let s2 = format!("{}{}", &s[..expect_index], &s[expect_base64_index..]);
            serde_json_lenient::from_str(&s2)
        } else {
            serde_json_lenient::from_str(s)
        })
        .context(format!("parse expect test result failed: {s}"))
    }
}

pub fn expect_failed_to_snapshot_result(
    pkg_src: &impl PackageSrcResolver,
    efr: ExpectFailedRaw,
) -> SnapshotResult {
    let filename = efr.loc.resolve(pkg_src).filename;
    let expect_file = dunce::canonicalize(PathBuf::from(&filename))
        .unwrap()
        .parent()
        .unwrap()
        .join("__snapshot__")
        .join(efr.expect.as_deref().unwrap_or_default());

    let file_content = if expect_file.exists() {
        Some(std::fs::read_to_string(&expect_file).unwrap())
    } else {
        None
    };
    let succ = match &file_content {
        Some(content) => content == efr.actual.as_deref().unwrap_or_default(),
        None => false,
    };
    SnapshotResult {
        loc: efr.loc,
        args_loc: efr.args_loc,
        expect_file: PathBuf::from(efr.expect.unwrap_or_default()),
        expect_content: file_content,
        actual: efr.actual.unwrap_or_default(),
        succ,
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SnapshotResult {
    pub loc: LocationJson,
    pub args_loc: Vec<Option<LocationJson>>,
    pub expect_file: PathBuf,
    pub expect_content: Option<String>,
    pub actual: String,
    pub succ: bool,
}

#[derive(Debug)]
struct Location {
    pub filename: String,
    pub line_start: u32,
    pub col_start: u32,
    pub line_end: u32,
    pub col_end: u32,
}

impl Location {
    fn ahead(&self, other: &Location) -> bool {
        (self.line_start, self.col_start, self.line_end, self.col_end)
            < (
                other.line_start,
                other.col_start,
                other.line_end,
                other.col_end,
            )
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}-{}:{}",
            self.filename,
            self.line_start + 1,
            self.col_start + 1,
            self.line_end + 1,
            self.col_end + 1,
        )
    }
}

#[derive(Debug)]
struct Replace {
    // inspect(1234)
    // ^............^
    pub loc: Location,

    // "1234"
    pub actual: String,
    // inspect(1234)
    //          ^..^
    pub actual_loc: Location,

    pub expect: String,
    pub expect_loc: Option<Location>,

    pub mode: Option<String>,
}

impl Replace {
    fn guess_target(&self) -> anyhow::Result<Target> {
        // trivial case where content is provided, we can get precise location
        if let Some(loc) = &self.expect_loc {
            Ok(Target {
                left_border_line_start: self.loc.line_start,
                left_border_col_start: self.loc.col_start,
                line_start: loc.line_start,
                col_start: loc.col_start,
                line_end: loc.line_end,
                col_end: loc.col_end,
                kind: TargetKind::Trivial,
                expect: self.expect.clone(),
                actual: self.actual.clone(),
                mode: self.mode.clone(),
            })
        } else {
            let is_pipe = self.actual_loc.ahead(&self.loc);
            if is_pipe {
                Ok(Target {
                    left_border_line_start: self.loc.line_start,
                    left_border_col_start: self.loc.col_start,
                    line_start: self.loc.line_end,
                    col_start: self.loc.col_end - 1,
                    line_end: self.loc.line_end,
                    col_end: self.loc.col_end - 1,
                    kind: TargetKind::Pipe,
                    expect: self.expect.clone(),
                    actual: self.actual.clone(),
                    mode: self.mode.clone(),
                })
            } else {
                // TODO: find comma
                Ok(Target {
                    left_border_line_start: self.actual_loc.line_end,
                    left_border_col_start: self.actual_loc.col_end,

                    line_start: self.loc.line_end,
                    col_start: self.loc.col_end - 1,
                    line_end: self.loc.line_end,
                    col_end: self.loc.col_end - 1,
                    kind: TargetKind::Call,
                    expect: self.expect.clone(),
                    actual: self.actual.clone(),
                    mode: self.mode.clone(),
                })
            }
        }
    }
}

fn base64_decode_string_codepoint(s: &str) -> String {
    let buf = base64::prelude::BASE64_STANDARD.decode(s).unwrap();
    let mut s = String::new();
    for i in (0..buf.len()).step_by(4) {
        let c = std::char::from_u32(
            (buf[i] as u32)
                | ((buf[i + 1] as u32) << 8)
                | ((buf[i + 2] as u32) << 16)
                | ((buf[i + 3] as u32) << 24),
        )
        .unwrap_or_else(|| panic!("Error: Invalid Unicode code point detected.\n\
             The following byte sequence does not represent a valid character: {:?}\n\
             The reason this happens may be that you constructed an invalid string in the MoonBit inspect test.\n",
            &buf[i..i + 4]));
        s.push(c);
    }
    s
}

#[test]
fn test_decode() {
    assert_eq!("", base64_decode_string_codepoint(""));
    assert_eq!("a", base64_decode_string_codepoint("YQAAAA=="));
    assert_eq!("ab", base64_decode_string_codepoint("YQAAAGIAAAA="));
    assert_eq!("abc", base64_decode_string_codepoint("YQAAAGIAAABjAAAA"));
    assert_eq!(
        "abcd",
        base64_decode_string_codepoint("YQAAAGIAAABjAAAAZAAAAA==")
    );
    assert_eq!(
        "abcde",
        base64_decode_string_codepoint("YQAAAGIAAABjAAAAZAAAAGUAAAA=")
    );
    assert_eq!("a中", base64_decode_string_codepoint("YQAAAC1OAAA="));
    assert_eq!("a中🤣", base64_decode_string_codepoint("YQAAAC1OAAAj+QEA"));
    assert_eq!(
        "a中🤣a",
        base64_decode_string_codepoint("YQAAAC1OAAAj+QEAYQAAAA==")
    );
    assert_eq!(
        "a中🤣中",
        base64_decode_string_codepoint("YQAAAC1OAAAj+QEALU4AAA==")
    );
}

fn parse_expect_failed_message(
    pkg_src: &impl PackageSrcResolver,
    msg: &str,
) -> anyhow::Result<Replace> {
    let j: ExpectFailedRaw = ExpectFailedRaw::from_str(msg)?;
    let locs: Vec<Option<Location>> = j
        .args_loc
        .iter()
        .map(|loc| loc.as_ref().map(|loc| loc.resolve(pkg_src)))
        .collect();
    if locs.len() != 4 {
        anyhow::bail!(
            "invalid locations {:?}, expect 4, got: {}",
            locs,
            locs.len()
        );
    }
    if locs[0].is_none() {
        // impossible
        anyhow::bail!("the location of first argument cannot be None");
    }
    let loc = j.loc.resolve(pkg_src);
    let mut locs_iter = locs.into_iter();
    let actual_loc = locs_iter.next().unwrap().unwrap();
    let expect_loc = locs_iter.next().unwrap();

    let expect = if let Some(base64) = &j.expect_base64 {
        base64_decode_string_codepoint(base64)
    } else {
        j.expect.unwrap_or_default().clone()
    };
    let actual = if let Some(base64) = &j.actual_base64 {
        base64_decode_string_codepoint(base64)
    } else {
        j.actual.unwrap_or_default().clone()
    };

    Ok(Replace {
        loc,
        expect,
        expect_loc,
        actual,
        actual_loc,
        mode: j.mode,
    })
}

impl LocationJson {
    fn resolve(&self, pkg_src: &impl PackageSrcResolver) -> Location {
        let actual_pkg = self.pkg.strip_suffix("_blackbox_test").unwrap_or(&self.pkg);
        let mut full_path = pkg_src.resolve_pkg_src(actual_pkg);
        full_path.push(&self.filename);
        Location {
            filename: full_path.display().to_string(),
            line_start: self.start_line - 1,
            col_start: self.start_column - 1,
            line_end: self.end_line - 1,
            col_end: self.end_column - 1,
        }
    }
}

fn collect<'a>(
    pkg_src: &impl PackageSrcResolver,
    messages: impl IntoIterator<Item = &'a str>,
) -> anyhow::Result<HashMap<String, BTreeSet<Target>>> {
    let mut targets: HashMap<String, BTreeSet<Target>> = HashMap::new();

    for msg in messages {
        if !msg.starts_with(EXPECT_FAILED) {
            continue;
        }
        let json_str = &msg[EXPECT_FAILED.len()..];
        let rep = parse_expect_failed_message(pkg_src, json_str)?;

        targets
            .entry(rep.loc.filename.clone())
            .or_default()
            .insert(rep.guess_target()?);
    }
    Ok(targets)
}

fn gen_patch(targets: HashMap<String, BTreeSet<Target>>) -> anyhow::Result<PackagePatch> {
    let mut pp = PackagePatch::default();
    for (filename, targets) in targets.into_iter() {
        let mut file_patches = vec![];
        let content = std::fs::read_to_string(&filename)?;
        let content_chars: Vec<char> = content.chars().collect();
        let line_index = line_index::LineIndex::new(&content);

        let charidx = content
            .char_indices()
            .enumerate()
            .collect::<Vec<(usize, (usize, char))>>();
        let mut byte_offset_to_char_offset = HashMap::new();
        for (i, (j, _c)) in charidx.iter() {
            byte_offset_to_char_offset.insert(*j, *i);
        }

        let lines = content.split('\n').collect::<Vec<&str>>();

        for t in targets.into_iter() {
            let offset_start =
                line_col_to_byte_idx(&line_index, t.line_start, t.col_start).unwrap();
            let offset_start =
                text_size::TextSize::new(byte_offset_to_char_offset[&offset_start] as u32);

            let offset_end = line_col_to_byte_idx(&line_index, t.line_end, t.col_end).unwrap();
            let offset_end =
                text_size::TextSize::new(byte_offset_to_char_offset[&offset_end] as u32);

            let mut rg = line_index::TextRange::new(offset_start, offset_end);

            let (left_padding, right_padding) = match t.kind {
                TargetKind::Trivial => {
                    let mut i = usize::from(offset_start);
                    let left_border_start = line_col_to_byte_idx(
                        &line_index,
                        t.left_border_line_start,
                        t.left_border_col_start,
                    )
                    .unwrap();
                    let left_border_start = byte_offset_to_char_offset[&left_border_start];
                    let mut find_lparen = false;

                    // CR: The left_border_start may be the start position of the `inspect(actual, content="...")` function call.
                    // trying to find the the left parenthesis `(` after the equal sign `=`.
                    while i >= left_border_start {
                        match content_chars[i] {
                            '=' => break,
                            '(' => {
                                find_lparen = true;
                                break;
                            }
                            _ => i -= 1,
                        }
                    }
                    // If the left parenthesis `(` is not found, parentheses need to be
                    // added around the multiline string content.
                    if !find_lparen {
                        (Some("(".to_string()), Some(")".to_string()))
                    } else {
                        (None, None)
                    }
                }
                TargetKind::Pipe => {
                    let mut find_paren = false;
                    let mut i = usize::from(offset_start);
                    let left_border_start = line_col_to_byte_idx(
                        &line_index,
                        t.left_border_line_start,
                        t.left_border_col_start,
                    )
                    .unwrap();
                    let left_border_start = byte_offset_to_char_offset[&left_border_start];

                    while i >= left_border_start {
                        let c = content_chars[i];
                        if c == ')' {
                            find_paren = true;
                            break;
                        }
                        i -= 1;
                    }
                    if find_paren {
                        (Some("content=".to_string()), None)
                    } else {
                        let offset_start =
                            line_col_to_byte_idx(&line_index, t.line_start, t.col_start + 1)
                                .unwrap();

                        let offset_start = text_size::TextSize::new(
                            byte_offset_to_char_offset[&offset_start] as u32,
                        );
                        let offset_end =
                            line_col_to_byte_idx(&line_index, t.line_end, t.col_end + 1).unwrap();

                        let offset_end = text_size::TextSize::new(
                            byte_offset_to_char_offset[&offset_end] as u32,
                        );

                        rg = line_index::TextRange::new(offset_start, offset_end);
                        (Some("(content=".to_string()), Some(")".to_string()))
                    }
                }
                TargetKind::Call => {
                    let mut i = usize::from(offset_start);
                    let left_border_start = line_col_to_byte_idx(
                        &line_index,
                        t.left_border_line_start,
                        t.left_border_col_start,
                    )
                    .unwrap();
                    let left_border_start = byte_offset_to_char_offset[&left_border_start];

                    let mut find_comma = false;
                    let mut find_lparen = false;
                    // CR: This approach is fragile and may fail in edge cases. For example:
                    //
                    // ```
                    //  inspect(123 // ,
                    //  )
                    // ```
                    while i >= left_border_start && !(find_comma && find_lparen) {
                        match content_chars[i] {
                            ',' => find_comma = true,
                            '(' => find_lparen = true,
                            _ => (),
                        }
                        i -= 1
                    }
                    let left_padding = if find_comma {
                        "content=("
                    } else {
                        ", content=("
                    }
                    .to_string();
                    let right_padding = ")".to_string();

                    (Some(left_padding), Some(right_padding))
                }
            };

            let is_doc_test = lines[(t.line_start as usize)..((t.line_end as usize) + 1)]
                .iter()
                .all(|line| line.starts_with("///"));

            // CR: unused loop?
            for line in lines[t.line_start as usize..].iter() {
                if line.trim().ends_with(")?")
                    && !line.trim().starts_with("#|")
                    && !line.trim().starts_with("$|")
                {
                    break;
                }
            }

            file_patches.push(BufferExpect {
                range: rg,
                line_start: t.line_start as i32,
                col_start: t.col_start as i32,
                line_end: t.line_end as i32,
                col_end: t.col_end as i32,
                left_padding,
                right_padding,
                expect: t.expect,
                actual: t.actual,
                kind: t.kind,
                mode: t.mode.clone(),
                is_doc_test,
            });
        }

        pp.patches.insert(filename.to_string(), file_patches);
    }
    Ok(pp)
}

fn push_multi_line_string(
    output: &mut String,
    spaces: usize,
    s: &str,
    prev_char: Option<&char>,
    next_char: Option<&char>,
    is_doc_test: bool,
) {
    let content = push_multi_line_string_internal(spaces, s, prev_char, next_char);
    if !is_doc_test {
        output.push_str(&content);
    } else {
        let lines: Vec<&str> = content.split('\n').collect();
        if !output.ends_with("\n") {
            output.push('\n');
        }
        for (i, line) in lines.iter().enumerate() {
            if line.trim().starts_with("#|") || line.trim().starts_with("$|") {
                let spaces = if i == 0 { 2 } else { 0 };
                output.push_str(&format!("/// {}{}\n", " ".repeat(spaces).as_str(), line));
            }
        }
        output.push_str("/// ");
    }
}

fn to_moonbit_multi_line_string(s: &str) -> String {
    format!("#|{s}")
}

fn push_multi_line_string_internal(
    spaces: usize,
    s: &str,
    prev_char: Option<&char>,
    next_char: Option<&char>,
) -> String {
    let mut output = String::new();
    let lines: Vec<&str> = s.split('\n').collect();
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            match prev_char {
                Some('=') | Some('(') => {
                    output.push('\n');
                    output.push_str(" ".repeat(spaces).as_str());
                }
                Some(c) if c.is_alphabetic() => {
                    output.push('\n');
                    output.push_str(" ".repeat(spaces).as_str());
                }
                _ => {}
            }
            output.push_str(&to_moonbit_multi_line_string(line));
        } else {
            match prev_char {
                Some(' ') => {
                    output.push_str(&format!(
                        "{}{}",
                        " ".repeat(spaces.saturating_sub(2)),
                        to_moonbit_multi_line_string(line),
                    ));
                }
                _ => {
                    output.push_str(&format!(
                        "{}{}",
                        " ".repeat(spaces),
                        to_moonbit_multi_line_string(line),
                    ));
                }
            }
        }
        if i == lines.len() - 1 {
            output.push('\n');
            if let Some(')' | ',') = next_char {
                output.push_str(" ".repeat(spaces.saturating_sub(2)).as_str());
            }
        } else {
            output.push('\n');
        }
    }
    output
}

fn apply_patch(pp: &PackagePatch) -> anyhow::Result<()> {
    for (filename, patches) in pp.patches.iter() {
        let content = std::fs::read_to_string(filename)?;
        // TODO: share content_chars with gen_patch
        let content_chars = content.chars().collect::<Vec<char>>();

        let charidx = content
            .char_indices()
            .enumerate()
            .collect::<Vec<(usize, (usize, char))>>();
        let mut char_offset_to_byte_offset = HashMap::new();
        for (i, (j, _c)) in charidx.iter() {
            char_offset_to_byte_offset.insert(*i, *j);
        }

        let lines: Vec<&str> = content.split('\n').collect();
        let line_index = line_index::LineIndex::new(&content);
        let mut output = String::new();
        let mut i = 0u32;
        let mut k = 0usize;
        while k < patches.len() {
            let is_doc_test = patches[k].is_doc_test;
            let patch = &patches[k];
            let start = patch.range.start();
            let end = patch.range.end();
            #[allow(clippy::comparison_chain)]
            if i < u32::from(start) {
                for item in content_chars
                    .iter()
                    .take(usize::from(start))
                    .skip(i as usize)
                {
                    output.push(*item);
                }
                i = u32::from(start);
            } else if i == u32::from(start) {
                // infer indent
                let utf8_start = char_offset_to_byte_offset[&usize::from(start)];
                let start_point = line_index.line_col(text_size::TextSize::new(utf8_start as u32));
                let line = lines[start_point.line as usize];
                let spaces = line.find(|c| c != ' ').unwrap_or(0);

                if let Some(padding) = &patch.left_padding {
                    output.push_str(padding);
                    if patch.kind == TargetKind::Call && patch.actual.contains('\n') {
                        output.push('\n');
                        if !is_doc_test {
                            output.push_str(" ".repeat(spaces + 2).as_str());
                        }
                    }
                }

                match patch.mode.as_deref() {
                    None => {
                        let next_char = content_chars[usize::from(end)..].first();
                        let prev_char = content_chars[..usize::from(start)].last();
                        push_multi_line_string(
                            &mut output,
                            spaces + 2,
                            &patch.actual,
                            prev_char,
                            next_char,
                            is_doc_test,
                        );
                    }
                    Some("json") => {
                        output.push_str(&patch.actual.to_string());
                    }
                    Some(mode) => {
                        anyhow::bail!("unsupported mode: {:?} in expect testing", mode);
                    }
                }

                if let Some(padding) = &patch.right_padding {
                    output.push_str(padding);
                }

                i = u32::from(end);
                k += 1;
            } else {
                anyhow::bail!("unreachable state in expect test, please report this bug");
            }
        }
        if i < content_chars.len() as u32 {
            for item in content_chars.into_iter().skip(i as usize) {
                output.push(item);
            }
        }
        std::fs::write(filename, output)?;
    }

    Ok(())
}

pub fn apply_snapshot<'a>(
    pkg_src: &impl PackageSrcResolver,
    messages: impl IntoIterator<Item = &'a str>,
) -> anyhow::Result<()> {
    let snapshots = messages
        .into_iter()
        .filter(|msg| msg.starts_with(SNAPSHOT_TESTING))
        .map(|msg| {
            let json_str = &msg[SNAPSHOT_TESTING.len()..];
            let rep: ExpectFailedRaw = ExpectFailedRaw::from_str(json_str)
                .context(format!("parse snapshot test result failed: {json_str}"))
                .unwrap();
            rep
        })
        .map(|epf| expect_failed_to_snapshot_result(pkg_src, epf));

    for snapshot in snapshots {
        let filename = snapshot.loc.resolve(pkg_src).filename;
        let actual = snapshot.actual.clone();
        let expect_file = &snapshot.expect_file;
        let expect_file = dunce::canonicalize(PathBuf::from(&filename))
            .unwrap()
            .parent()
            .unwrap()
            .join("__snapshot__")
            .join(expect_file);

        if !expect_file.parent().unwrap().exists() {
            std::fs::create_dir_all(expect_file.parent().unwrap())?;
        }
        let expect = if expect_file.exists() {
            std::fs::read_to_string(&expect_file)?
        } else {
            "".to_string()
        };
        if actual != expect {
            std::fs::write(&expect_file, actual)?;
        }
    }

    Ok(())
}

pub fn apply_expect<'a>(
    pkg_src: &impl PackageSrcResolver,
    messages: impl IntoIterator<Item = &'a str>,
) -> anyhow::Result<()> {
    // dbg!(&messages);
    let targets = collect(pkg_src, messages)?;
    // dbg!(&targets);
    let patches = gen_patch(targets)?;
    // dbg!(&patches);
    apply_patch(&patches)?;
    Ok(())
}

/// The context size for unified diff output.
const DIFF_MIN_CTX: usize = 5;

/// The minimum number of lines to trigger chunking in diff output.
///
/// If input strings have fewer lines than this, the entire diff will be shown
/// without chunking.
const MIN_LINES_FOR_CHUNKING: usize = 20;

/// Write the difference between strings `expected` and `actual` to the provided
/// writer `to`, in unified diff format.
///
/// The function uses a number of heuristics to determine the best format for
/// the diff output. Inline diffs will not show newline differences, short diffs
/// will be displayed in full, and long diffs will be chunked.
pub fn write_diff(
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
        writeln!(to, "{}", header.to_string().bright_black())?;
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
                        write!(to, "{}", "-".bright_red())?;
                        for &(emph, slice) in change.values() {
                            if emph {
                                write!(to, "{}", slice.underline().bright_red())?;
                            } else {
                                write!(to, "{}", slice.bright_red())?;
                            }
                        }
                    }
                    similar::ChangeTag::Insert => {
                        write!(to, "{}", "+".bright_green())?;
                        for &(emph, slice) in change.values() {
                            if emph {
                                write!(to, "{}", slice.underline().bright_green())?;
                            } else {
                                write!(to, "{}", slice.bright_green())?;
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
            writeln!(to, "{}", "\\ No newline at end of file".dimmed())?;
        }
    }
    Ok(())
}

fn write_diff_header(mut to: impl Write) -> std::io::Result<()> {
    writeln!(
        to,
        "{} ({}, {})",
        "Diff:".bold(),
        "- expected".red(),
        "+ actual".green()
    )
}

pub fn render_expect_fail(pkg_src: &impl PackageSrcResolver, msg: &str) -> anyhow::Result<()> {
    assert!(msg.starts_with(EXPECT_FAILED));
    let json_str = &msg[EXPECT_FAILED.len()..];
    let rep = parse_expect_failed_message(pkg_src, json_str)?;

    if let Some("json") = rep.mode.as_deref() {
        let j_expect = serde_json_lenient::from_str(&rep.expect)?;
        let j_actual = serde_json_lenient::from_str(&rep.actual)?;
        let diffs = json_structural_diff::JsonDiff::diff(&j_expect, &j_actual, false);
        if let Some(diff) = diffs.diff {
            let diffs = json_structural_diff::colorize(&diff, true);
            println!("inspect failed at {}", rep.loc);
            println!("{}", "Diff:".bold());
            println!("{diffs}");
        }
        return Ok(());
    }

    println!("expect test failed at {}", rep.loc);
    write_diff_header(std::io::stdout())?;
    println!("----");
    write_diff(
        &rep.expect,
        &rep.actual,
        MIN_LINES_FOR_CHUNKING,
        DIFF_MIN_CTX,
        std::io::stdout(),
    )?;
    println!("----");
    println!();

    Ok(())
}

pub fn snapshot_eq(pkg_src: &impl PackageSrcResolver, msg: &str) -> anyhow::Result<bool> {
    assert!(msg.starts_with(SNAPSHOT_TESTING));
    let json_str = &msg[SNAPSHOT_TESTING.len()..];

    let e: ExpectFailedRaw = ExpectFailedRaw::from_str(json_str)
        .context(format!("parse snapshot test result failed: {json_str}"))?;
    let snapshot = expect_failed_to_snapshot_result(pkg_src, e);

    let filename = snapshot.loc.resolve(pkg_src).filename;
    let actual = snapshot.actual.clone();
    let expect_file = &snapshot.expect_file;
    let expect_file = dunce::canonicalize(PathBuf::from(&filename))
        .unwrap()
        .parent()
        .unwrap()
        .join("__snapshot__")
        .join(expect_file);

    let expect = if expect_file.exists() {
        std::fs::read_to_string(&expect_file)?
    } else {
        "".to_string()
    };
    fn normalize_string(s: &str) -> String {
        s.replace("\r\n", "\n").replace('\r', "")
    }
    let eq = normalize_string(&actual) == normalize_string(&expect);
    Ok(eq)
}

pub fn render_snapshot_fail(
    pkg_src: &impl PackageSrcResolver,
    msg: &str,
) -> anyhow::Result<(bool, String, String)> {
    assert!(msg.starts_with(SNAPSHOT_TESTING));
    let json_str = &msg[SNAPSHOT_TESTING.len()..];

    let e: ExpectFailedRaw = ExpectFailedRaw::from_str(json_str)
        .context(format!("parse snapshot test result failed: {json_str}"))?;
    let snapshot = expect_failed_to_snapshot_result(pkg_src, e);

    let loc = snapshot.loc.resolve(pkg_src);
    let actual = snapshot.actual.clone();
    let expect_file = &snapshot.expect_file;
    let expect_file = dunce::canonicalize(PathBuf::from(&loc.filename))
        .unwrap()
        .parent()
        .unwrap()
        .join("__snapshot__")
        .join(expect_file);

    let expect = if expect_file.exists() {
        std::fs::read_to_string(&expect_file)?
    } else {
        "".to_string()
    };
    let eq = actual == expect;
    if !eq {
        println!(
            "expect test failed at {}:{}:{}",
            loc.filename,
            loc.line_start + 1,
            loc.col_start + 1
        );
        write_diff_header(std::io::stdout())?;
        println!("----");
        write_diff(
            &expect,
            &actual,
            MIN_LINES_FOR_CHUNKING,
            DIFF_MIN_CTX,
            std::io::stdout(),
        )?;
        println!("----");
        println!();
    }
    Ok((eq, expect, actual))
}

#[test]
fn test_split() {
    let input = "\n";
    let xs: Vec<&str> = input.split('\n').collect();
    assert_eq!(xs, ["", ""]);

    let input = "\n\n";
    let xs: Vec<&str> = input.split('\n').collect();
    assert_eq!(xs, ["", "", ""]);
}

#[test]
fn test_x() {
    let input = r#"fn actual() -> String {
  "BinOp('+', BinOp('+', Num(1), Num(2)), Num(3))"
}

test {
  inspect(actual(), content="BinOp('+', Num(1), Num(2))")?
}
"#;
    let chars = input.char_indices().collect::<Vec<(usize, char)>>();
    for (i, c) in chars {
        println!("{i}: {c}");
    }
}
