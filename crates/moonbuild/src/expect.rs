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
use moonutil::common::line_col_to_byte_idx;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

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

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ExpectFailedRaw {
    pub loc: String,
    pub args_loc: String,
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

pub fn expect_failed_to_snapshot_result(efr: ExpectFailedRaw) -> SnapshotResult {
    let filename = parse_filename(&efr.loc).unwrap();
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

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SnapshotResult {
    pub loc: String,
    pub args_loc: String,
    pub expect_file: PathBuf,
    pub expect_content: Option<String>,
    pub actual: String,
    pub succ: bool,
}

#[derive(Debug)]
struct Location {
    pub raw: String,
    pub line_start: u32,
    pub col_start: u32,
    pub line_end: u32,
    pub col_end: u32,
}

impl Location {
    pub fn ahead(&self, other: &Location) -> bool {
        (self.line_start, self.col_start, self.line_end, self.col_end)
            < (
                other.line_start,
                other.col_start,
                other.line_end,
                other.col_end,
            )
    }
}

#[derive(Debug)]
struct Replace {
    pub filename: String,

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

struct EscapeInfo {
    pub quote: bool,
    pub newline: bool,
    pub ascii_control: bool,
}

fn detect_escape_info(s: &str) -> EscapeInfo {
    let mut info = EscapeInfo {
        quote: false,
        newline: false,
        ascii_control: false,
    };

    for c in s.chars() {
        match c {
            '"' => info.quote = true,
            '\n' => info.newline = true,
            c if (c as u32) < 0x20 => info.ascii_control = true,
            _ => {}
        }
    }
    info
}

fn to_moonbit_style(s: &str, with_quote: bool) -> String {
    let mut buf = String::new();
    if with_quote {
        buf.push('"');
    }
    for c in s.chars() {
        match c {
            c if (c as u32) < 0x20 => {
                buf.push_str(&format!("\\u{{{:x}}}", c as u32));
            }
            '"' => buf.push_str("\\\""),
            '\\' => buf.push_str("\\\\"),
            '\n' => buf.push_str("\\n"),
            '\r' => buf.push_str("\\r"),
            '\t' => buf.push_str("\\t"),
            _ => buf.push(c),
        }
    }
    if with_quote {
        buf.push('"');
    }
    buf
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

fn parse_expect_failed_message(msg: &str) -> anyhow::Result<Replace> {
    let j: ExpectFailedRaw = ExpectFailedRaw::from_str(msg)?;
    let locs: Vec<Option<String>> = serde_json_lenient::from_str(&j.args_loc)?;
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
    let loc = parse_loc(&j.loc)?;
    let actual_loc = parse_loc(locs[0].as_ref().unwrap())?;
    let expect_loc = if locs[1].is_some() {
        Some(parse_loc(locs[1].as_ref().unwrap())?)
    } else {
        None
    };

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
        filename: parse_filename(&j.loc)?,
        loc,
        expect,
        expect_loc,
        actual,
        actual_loc,
        mode: j.mode,
    })
}

pub fn parse_filename(loc: &str) -> anyhow::Result<String> {
    let mut index = loc.len();
    let mut colon = 0;
    for (i, c) in loc.char_indices().rev() {
        if c == ':' {
            colon += 1;
            if colon == 3 {
                index = i;
                break;
            }
        }
    }
    Ok(loc[..index].to_string())
}

fn parse_loc(loc: &str) -> anyhow::Result<Location> {
    // find 3rd colon from right of loc
    let mut index = loc.len();
    let mut colon = 0;
    for (i, c) in loc.char_indices().rev() {
        if c == ':' {
            colon += 1;
            if colon == 3 {
                index = i;
                break;
            }
        }
    }
    let tmp = &loc[index + 1..];
    let rloc = tmp.replace('-', ":");
    let parts: Vec<&str> = rloc.split(':').collect();
    if parts.len() != 4 {
        anyhow::bail!("invalid location: {}", rloc);
    }
    let line_start = parts[0].parse::<u32>()? - 1;
    let col_start = parts[1].parse::<u32>()? - 1;
    let line_end = parts[2].parse::<u32>()? - 1;
    let col_end = parts[3].parse::<u32>()? - 1;
    Ok(Location {
        raw: loc.to_string(),
        line_start,
        col_start,
        line_end,
        col_end,
    })
}

fn collect<'a>(
    messages: impl IntoIterator<Item = &'a str>,
) -> anyhow::Result<HashMap<String, BTreeSet<Target>>> {
    let mut targets: HashMap<String, BTreeSet<Target>> = HashMap::new();

    for msg in messages {
        if !msg.starts_with(EXPECT_FAILED) {
            continue;
        }
        let json_str = &msg[EXPECT_FAILED.len()..];
        let rep = parse_expect_failed_message(json_str)?;

        match targets.get_mut(&rep.filename) {
            Some(st) => {
                st.insert(rep.guess_target()?);
            }
            None => {
                let mut newst = BTreeSet::new();
                newst.insert(rep.guess_target()?);
                targets.insert(rep.filename.clone(), newst);
            }
        }
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
                    let escape_info = detect_escape_info(&t.actual);
                    let is_double_quoted_string =
                        (!escape_info.newline && !escape_info.quote) || escape_info.ascii_control;

                    // If the left parenthesis `(` is not found, and the promoted content is a multi-line string,
                    // parentheses `(` and `)` need to be added around the promoted content.
                    if !find_lparen && !is_double_quoted_string {
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
                    let mut left_padding = String::new();
                    let mut right_padding = String::new();
                    left_padding.push_str(if find_comma { "content=" } else { ", content=" });
                    let escape_info = detect_escape_info(&t.actual);
                    let is_double_quoted_string =
                        (!escape_info.newline && !escape_info.quote) || escape_info.ascii_control;
                    if !is_double_quoted_string {
                        left_padding.push('(');
                        right_padding.push(')');
                    }

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
                        let escape_info = detect_escape_info(&patch.actual);
                        if (!escape_info.newline && !escape_info.quote) || escape_info.ascii_control
                        {
                            output.push_str(&to_moonbit_style(&patch.actual, true));
                        } else {
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

pub fn apply_snapshot<'a>(messages: impl IntoIterator<Item = &'a str>) -> anyhow::Result<()> {
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
        .map(expect_failed_to_snapshot_result);

    for snapshot in snapshots {
        let filename = parse_filename(&snapshot.loc)?;
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

pub fn apply_expect<'a>(messages: impl IntoIterator<Item = &'a str>) -> anyhow::Result<()> {
    // dbg!(&messages);
    let targets = collect(messages)?;
    // dbg!(&targets);
    let patches = gen_patch(targets)?;
    // dbg!(&patches);
    apply_patch(&patches)?;
    Ok(())
}

fn format_chunks(chunks: Vec<dissimilar::Chunk>) -> String {
    let mut buf = String::new();
    for chunk in chunks {
        let formatted = match chunk {
            dissimilar::Chunk::Equal(text) => text.into(),
            dissimilar::Chunk::Delete(text) => format!("{}", text.red().underline()),
            dissimilar::Chunk::Insert(text) => format!("{}", text.green().underline()),
        };
        buf.push_str(&formatted);
    }
    buf
}

pub fn render_expect_fail(msg: &str) -> anyhow::Result<()> {
    assert!(msg.starts_with(EXPECT_FAILED));
    let json_str = &msg[EXPECT_FAILED.len()..];
    let rep = parse_expect_failed_message(json_str)?;

    if let Some("json") = rep.mode.as_deref() {
        let j_expect = serde_json_lenient::from_str(&rep.expect)?;
        let j_actual = serde_json_lenient::from_str(&rep.actual)?;
        let diffs = json_structural_diff::JsonDiff::diff(&j_expect, &j_actual, false);
        if let Some(diff) = diffs.diff {
            let diffs = json_structural_diff::colorize(&diff, true);
            println!("inspect failed at {}", rep.loc.raw);
            println!("{}", "Diff:".bold());
            println!("{diffs}");
        }
        return Ok(());
    }

    let d = dissimilar::diff(&rep.expect, &rep.actual);
    println!(
        r#"expect test failed at {}
{}
----
{}
----
"#,
        rep.loc.raw,
        "Diff:".bold(),
        format_chunks(d)
    );
    Ok(())
}

pub fn snapshot_eq(msg: &str) -> anyhow::Result<bool> {
    assert!(msg.starts_with(SNAPSHOT_TESTING));
    let json_str = &msg[SNAPSHOT_TESTING.len()..];

    let e: ExpectFailedRaw = ExpectFailedRaw::from_str(json_str)
        .context(format!("parse snapshot test result failed: {json_str}"))?;
    let snapshot = expect_failed_to_snapshot_result(e);

    let filename = parse_filename(&snapshot.loc)?;
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

pub fn render_snapshot_fail(msg: &str) -> anyhow::Result<(bool, String, String)> {
    assert!(msg.starts_with(SNAPSHOT_TESTING));
    let json_str = &msg[SNAPSHOT_TESTING.len()..];

    let e: ExpectFailedRaw = ExpectFailedRaw::from_str(json_str)
        .context(format!("parse snapshot test result failed: {json_str}"))?;
    let snapshot = expect_failed_to_snapshot_result(e);

    let filename = parse_filename(&snapshot.loc)?;
    let loc = parse_loc(&snapshot.loc)?;
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
    let eq = actual == expect;
    if !eq {
        let d = dissimilar::diff(&expect, &actual);
        println!(
            r#"expect test failed at {}:{}:{}
{}
----
{}
----
"#,
            filename,
            loc.line_start + 1,
            loc.col_start + 1,
            "Diff:".bold(),
            format_chunks(d)
        );
    }
    Ok((eq, expect, actual))
}

pub fn render_expect_fails(messages: &[String]) -> anyhow::Result<()> {
    for msg in messages {
        if !msg.starts_with(EXPECT_FAILED) {
            continue;
        }
        render_expect_fail(msg)?;
    }
    Ok(())
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
