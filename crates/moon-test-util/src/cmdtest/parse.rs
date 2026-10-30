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

use std::path::Path;

#[derive(Debug)]
pub(crate) enum Block {
    Command {
        cmd: String,
        content: Option<String>,
    },
    Other {
        content: String,
    },
}

#[derive(Debug)]
enum LineMarker {
    Command,
    Content,
    Other,
}

pub(crate) fn parse(p: &Path) -> Vec<Block> {
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
