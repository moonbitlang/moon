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

#[derive(Debug)]
pub enum Block {
    Command {
        cmd: String,
        content: Option<String>,
    },
    Other {
        content: String,
    },
}

#[derive(Debug)]
pub enum LineMarker {
    Command,
    Content,
    Other,
}

pub fn parse(p: &Path) -> Vec<Block> {
    let content = std::fs::read_to_string(p).unwrap();
    let content = content.replace("\r\n", "\n");

    let lines: Vec<&str> = content.split('\n').collect();
    let markers: Vec<LineMarker> = lines
        .iter()
        .map(|line| {
            if line.starts_with("  $ ") {
                LineMarker::Command
            } else if line.starts_with("  ") {
                LineMarker::Content
            } else {
                LineMarker::Other
            }
        })
        .collect();

    // dbg!(&lines);
    // dbg!(&markers);

    let mut items: Vec<Block> = Vec::new();
    let mut cur_cmd: Option<String> = None;
    let mut cur_txt: Option<String> = None;

    let mut i = 0;
    while i < lines.len() {
        match markers[i] {
            LineMarker::Command => {
                if let Some(cmd) = cur_cmd.take() {
                    items.push(Block::Command {
                        cmd: cmd.replace("  $ ", "").trim().into(),
                        content: cur_txt.take(),
                    });
                }
                cur_cmd = Some(lines[i].to_string());
            }
            LineMarker::Content => {
                if let Some(txt) = cur_txt.as_mut() {
                    txt.push('\n');
                    txt.push_str(&lines[i][2..]);
                } else {
                    cur_txt = Some(lines[i][2..].to_string());
                }
            }
            LineMarker::Other => {
                items.push(Block::Other {
                    content: lines[i].to_string(),
                });
            }
        }
        i += 1;
    }

    if let Some(cmd) = cur_cmd {
        items.push(Block::Command {
            cmd: cmd.replace("  $ ", "").trim().into(),
            content: cur_txt,
        });
    }

    items
}
