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

use std::path::PathBuf;

#[test]
fn cmd_test() {
    let mut test_cases = Vec::new();

    for entry in
        walkdir::WalkDir::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases"))
    {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                eprintln!("Error reading entry: {}", err);
                continue;
            }
        };

        if entry.file_type().is_file() && entry.file_name() == "moon.test" {
            test_cases.push(entry.path().display().to_string());
        }
    }

    // execute `cargo xtask cmdtest $file`
    // if UPDATE_EXPECT been set, execute `cargo xtask cmdtest $file -u`
    for test_case in test_cases {
        let mut cmd = std::process::Command::new("cargo");
        cmd.arg("xtask").arg("cmdtest").arg(test_case);
        if std::env::var("UPDATE_EXPECT").is_ok() {
            cmd.arg("-u");
        }
        let status = cmd.status().unwrap();
        assert!(status.success());
    }
}
