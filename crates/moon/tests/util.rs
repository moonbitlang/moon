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

use std::path::{Path, PathBuf};

use expect_test::Expect;
use moonutil::{common::StringExt, compiler_flags::CC};

pub fn check<S: AsRef<str>>(actual: S, expect: Expect) {
    expect.assert_eq(actual.as_ref())
}

pub fn moon_bin() -> PathBuf {
    snapbox::cmd::cargo_bin("moon")
}

pub fn replace_dir(s: &str, dir: impl AsRef<std::path::Path>) -> String {
    let path_str1 = dunce::canonicalize(dir)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    // for something like "{...\"loc\":{\"path\":\"C:\\\\Users\\\\runneradmin\\\\AppData\\\\Local\\\\Temp\\\\.tmpP0u4VZ\\\\main\\\\main.mbt\"...\r\n" on windows
    // https://github.com/moonbitlang/moon/actions/runs/10092428950/job/27906057649#step:13:149
    let s = s.replace("\\\\", "\\");
    let s = s.replace(&path_str1, "$ROOT");
    let s = s.replace(
        dunce::canonicalize(moonutil::moon_dir::home())
            .unwrap()
            .to_str()
            .unwrap(),
        "$MOON_HOME",
    );
    let cc_path = CC::default().cc_path;
    let ar_path = CC::default().ar_path;
    let s = s.replace(&ar_path, CC::default().ar_name());
    let s = s.replace(&cc_path, CC::default().cc_name());
    let s = s.replace(moon_bin().to_string_lossy().as_ref(), "moon");
    s.replace("\r\n", "\n").replace('\\', "/")
}

pub fn copy(src: &Path, dest: &Path) -> anyhow::Result<()> {
    if src.is_dir() {
        if !dest.exists() {
            std::fs::create_dir_all(dest)?;
        }
        for entry in walkdir::WalkDir::new(src) {
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

#[track_caller]
pub fn read<P: AsRef<Path>>(p: P) -> String {
    std::fs::read_to_string(p).unwrap().replace_crlf_to_lf()
}
