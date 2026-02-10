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
use snapbox::assert::Action;

use super::parse;

fn copy(src: &Path, dest: &Path) -> anyhow::Result<()> {
    if src.is_dir() {
        if !dest.exists() {
            std::fs::create_dir_all(dest)?;
        }
        let mut builder = ignore::WalkBuilder::new(src);
        builder.hidden(false);
        builder.git_global(false);
        for entry in builder.build() {
            let entry = entry?;
            let path = entry.path();
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

pub fn t(file: &Path, moon_bin: &Path, update: bool) -> i32 {
    let p = dunce::canonicalize(file).unwrap();
    let tmpdir = tempfile::tempdir().unwrap();
    let folder_name = p
        .parent()
        .unwrap()
        .components()
        .rev()
        .peekable()
        .peek()
        .unwrap()
        .as_os_str();
    copy(file.parent().unwrap(), &tmpdir.path().join(folder_name)).unwrap();
    let workdir = dunce::canonicalize(tmpdir.path().join(folder_name)).unwrap();

    let items = parse::parse(file);
    let mut result: Vec<parse::Block> = vec![];

    for item in items.iter() {
        match item {
            parse::Block::Command { cmd, content: _ } => {
                let args = cmd.split_whitespace().collect::<Vec<&str>>();
                let exec = construct_executable(args[0], moon_bin);
                let ret = exec.execute(&args[1..], &workdir);
                let actual = ret.normalize(&workdir, moon_bin);
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

    let mut actual = String::new();
    for item in result.iter() {
        match item {
            parse::Block::Command { cmd, content } => {
                actual.push_str("  $ ");
                actual.push_str(cmd);
                actual.push('\n');
                if let Some(content) = content {
                    for (i, line) in content.split('\n').enumerate() {
                        if i > 0 {
                            actual.push('\n');
                        }
                        actual.push_str("  ");
                        actual.push_str(line);
                    }
                    actual.push('\n');
                }
            }
            parse::Block::Other { content } => actual.push_str(content),
        }
    }

    let expected = snapbox::Data::read_from(file, None);
    let assertion = snapbox::Assert::new().action(if update {
        Action::Overwrite
    } else {
        Action::Verify
    });

    match assertion.try_eq(Some(&file.display()), snapbox::Data::text(actual), expected) {
        Ok(_) => 0,
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}
