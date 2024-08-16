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
use colored::Colorize;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::PathBuf;

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
pub struct BufferExpect {
    range: line_index::TextRange,
    left_padding: Option<&'static str>,
    right_padding: Option<&'static str>,
    #[allow(unused)]
    expect: String,
    actual: String,
    kind: TargetKind,
}

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
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ExpectFailedRaw {
    pub loc: String,
    pub args_loc: String,
    pub expect: String,
    pub actual: String,
    pub snapshot: Option<bool>,
}

pub fn expect_failed_to_snapshot_result(efr: ExpectFailedRaw) -> SnapshotResult {
    let filename = parse_filename(&efr.loc).unwrap();
    let expect_file = PathBuf::from(&filename)
        .canonicalize()
        .unwrap()
        .parent()
        .unwrap()
        .join("__snapshot__")
        .join(&efr.expect);

    let file_content = if expect_file.exists() {
        Some(std::fs::read_to_string(&expect_file).unwrap())
    } else {
        None
    };
    let succ = match &file_content {
        Some(content) => content == &efr.actual,
        None => false,
    };
    SnapshotResult {
        loc: efr.loc,
        args_loc: efr.args_loc,
        expect_file: PathBuf::from(efr.expect),
        expect_content: file_content,
        actual: efr.actual,
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

    // inspect(1234)?
    // ^...........^
    pub loc: Location,

    // "1234"
    pub actual: String,
    // inspect(1234)?
    //         ^..^
    pub actual_loc: Location,

    pub expect: String,
    pub expect_loc: Option<Location>,
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
                })
            }
        }
    }
}

fn line_col_to_byte_idx(line_index: &line_index::LineIndex, line: u32, col: u32) -> Option<usize> {
    let offset = line_index.offset(line_index.to_utf8(
        line_index::WideEncoding::Utf32,
        line_index::WideLineCol { line, col },
    )?)?;
    Some(usize::from(offset))
}

fn parse_expect_failed_message(msg: &str) -> anyhow::Result<Replace> {
    let j: ExpectFailedRaw = serde_json_lenient::from_str(msg)
        .context(format!("parse expect test result failed: {}", msg))?;
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
    Ok(Replace {
        filename: parse_filename(&j.loc)?,
        loc,
        expect: j.expect,
        expect_loc,
        actual: j.actual,
        actual_loc,
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

fn collect(messages: &[String]) -> anyhow::Result<HashMap<String, BTreeSet<Target>>> {
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
                TargetKind::Trivial => (None, None),
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
                        (Some("content="), None)
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
                        (Some("(content="), Some(")"))
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
                    while i >= left_border_start {
                        if content_chars[i] == ',' {
                            find_comma = true;
                            break;
                        }
                        i -= 1;
                    }
                    if find_comma {
                        (Some("content="), None)
                    } else {
                        (Some(", content="), None)
                    }
                }
            };

            for line in lines[t.line_start as usize..].iter() {
                if line.trim().ends_with(")?") && !line.trim().starts_with("#|") {
                    break;
                }
            }
            file_patches.push(BufferExpect {
                range: rg,
                left_padding,
                right_padding,
                expect: t.expect,
                actual: t.actual,
                kind: t.kind,
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
) {
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
            output.push_str(&format!("#|{}", line));
        } else {
            match prev_char {
                Some(' ') => {
                    output.push_str(&format!(
                        "{}#|{}",
                        " ".repeat(if spaces > 2 { spaces - 2 } else { 0 }),
                        line
                    ));
                }
                _ => {
                    output.push_str(&format!("{}#|{}", " ".repeat(spaces), line));
                }
            }
        }
        if i == lines.len() - 1 {
            if let Some(')') = next_char {
                output.push('\n');
                output.push_str(" ".repeat(if spaces > 2 { spaces - 2 } else { 0 }).as_str());
            }
        } else {
            output.push('\n');
        }
    }
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

                if let Some(padding) = patch.left_padding {
                    output.push_str(padding);
                    if patch.kind == TargetKind::Call && patch.actual.contains('\n') {
                        output.push('\n');
                        output.push_str(" ".repeat(spaces + 2).as_str());
                    }
                }

                if !patch.actual.contains('\n') && !patch.actual.contains('"') {
                    output.push_str(&format!("{:?}", &patch.actual));
                } else {
                    let next_char = content_chars[usize::from(end)..].first();
                    let prev_char = content_chars[..usize::from(start)].last();
                    push_multi_line_string(
                        &mut output,
                        spaces + 2,
                        &patch.actual,
                        prev_char,
                        next_char,
                    );
                }

                if let Some(padding) = patch.right_padding {
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

pub fn apply_snapshot(messages: &[String]) -> anyhow::Result<()> {
    let snapshots: Vec<SnapshotResult> = messages
        .iter()
        .filter(|msg| msg.starts_with(SNAPSHOT_TESTING))
        .map(|msg| {
            let json_str = &msg[SNAPSHOT_TESTING.len()..];
            let rep: ExpectFailedRaw = serde_json_lenient::from_str(json_str)
                .context(format!("parse snapshot test result failed: {}", json_str))
                .unwrap();
            rep
        })
        .map(expect_failed_to_snapshot_result)
        .collect();

    for snapshot in snapshots.iter() {
        let filename = parse_filename(&snapshot.loc)?;
        let loc = parse_loc(&snapshot.loc)?;
        let actual = snapshot.actual.clone();
        let expect_file = &snapshot.expect_file;
        let expect_file = PathBuf::from(&filename)
            .canonicalize()
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
            std::fs::write(&expect_file, actual)?;
        }
    }

    Ok(())
}

pub fn apply_expect(messages: &[String]) -> anyhow::Result<()> {
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

pub fn render_snapshot_fail(msg: &str) -> anyhow::Result<()> {
    assert!(msg.starts_with(SNAPSHOT_TESTING));
    let json_str = &msg[SNAPSHOT_TESTING.len()..];

    let e: ExpectFailedRaw = serde_json_lenient::from_str(json_str)
        .context(format!("parse snapshot test result failed: {}", json_str))?;
    let snapshot = expect_failed_to_snapshot_result(e);

    let filename = parse_filename(&snapshot.loc)?;
    let loc = parse_loc(&snapshot.loc)?;
    let actual = snapshot.actual.clone();
    let expect_file = &snapshot.expect_file;
    let expect_file = PathBuf::from(&filename)
        .canonicalize()
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
    if actual != expect {
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
    Ok(())
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
        println!("{}: {}", i, c);
    }
}
