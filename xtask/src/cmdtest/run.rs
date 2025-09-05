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

use std::path::Path;

use crate::cmdtest::exec::construct_executable;

use super::parse;

fn format_chunks(chunks: Vec<dissimilar::Chunk>) -> String {
    let mut buf = String::new();
    for chunk in chunks {
        let formatted = match chunk {
            dissimilar::Chunk::Equal(text) => text.into(),
            dissimilar::Chunk::Delete(text) => format!("\x1b[4m\x1b[31m{}\x1b[0m", text),
            dissimilar::Chunk::Insert(text) => format!("\x1b[4m\x1b[32m{}\x1b[0m", text),
        };
        buf.push_str(&formatted);
    }
    buf
}

pub fn render_expect_fail(cmd: &str, expected: &str, actual: &str) {
    let diff = dissimilar::diff(expected, actual);

    println!(
        "\n
\x1b[1m\x1b[91merror\x1b[97m: expect test failed\x1b[0m
   \x1b[1m\x1b[34m-->\x1b[0m {}
\x1b[1mExpect\x1b[0m:
----
{}
----

\x1b[1mActual\x1b[0m:
----
{}
----

\x1b[1mDiff\x1b[0m:
----
{}
----
",
        cmd,
        expected,
        actual,
        format_chunks(diff)
    );
}

fn copy(src: &Path, dest: &Path) -> anyhow::Result<()> {
    if src.is_dir() {
        if !dest.exists() {
            std::fs::create_dir_all(dest)?;
        }
        let mut walk_dir = walkdir::WalkDir::new(src).into_iter();
        while let Some(entry) = walk_dir.next() {
            let entry = entry?;
            let path = entry.path();

            if path.ends_with("target") {
                walk_dir.skip_current_dir();
                continue;
            }

            let relative_path = path.strip_prefix(src)?;
            let dest_path = dest.join(relative_path);

            if path.is_dir() {
                if !dest_path.exists() {
                    std::fs::create_dir_all(dest_path)?;
                }
            } else {
                std::fs::copy(path, dest_path)?;
            }
        }
    } else {
        std::fs::copy(src, dest)?;
    }
    Ok(())
}

pub fn t(file: &Path, update: bool) -> i32 {
    let p = dunce::canonicalize(file).unwrap();
    let tmpdir = tempfile::tempdir().unwrap();
    let folder_name = p
        .parent()
        .unwrap()
        .components()
        .into_iter()
        .rev()
        .peekable()
        .peek()
        .unwrap()
        .as_os_str();
    copy(file.parent().unwrap(), &tmpdir.path().join(folder_name)).unwrap();
    let workdir = dunce::canonicalize(tmpdir.path().join(folder_name)).unwrap();

    let items = parse::parse(file);
    // dbg!(&items);
    let mut result: Vec<parse::Block> = vec![];

    let mut exit_code = 0;

    for item in items.iter() {
        match item {
            parse::Block::Command { cmd, content } => {
                let args = cmd.split_whitespace().collect::<Vec<&str>>();
                let expect = content.as_deref().unwrap_or_default();
                let exec = construct_executable(args[0]);
                let ret = exec.execute(&args[1..], &workdir);
                let actual = ret.normalize(&workdir);

                // dbg!(&expect, &actual);

                if expect != actual {
                    render_expect_fail(cmd, expect, &actual);
                    exit_code = 1;
                }
                result.push(parse::Block::Command {
                    cmd: cmd.to_string(),
                    content: Some(actual),
                });
            }
            parse::Block::Other { content } => result.push(parse::Block::Other {
                content: content.clone(),
            }),
        }
    }

    let mut buf = String::new();
    for item in result.iter() {
        match item {
            parse::Block::Command { cmd, content } => {
                buf.push_str("  $ ");
                buf.push_str(cmd);
                buf.push_str("\n");
                if let Some(content) = content {
                    for (i, line) in content.split('\n').enumerate() {
                        if i > 0 {
                            buf.push('\n');
                        }
                        buf.push_str("  ");
                        buf.push_str(line);
                    }
                    buf.push('\n');
                }
            }
            parse::Block::Other { content } => buf.push_str(content),
        }
    }
    if update {
        std::fs::write(file, buf).unwrap();
        exit_code = 0;
    }
    exit_code
}

#[test]
fn test() {
    let s = "a\n";
    let lines0: Vec<&str> = s.lines().collect();
    let lines1: Vec<&str> = s.split('\n').collect();
    dbg!(lines0, lines1);
}
