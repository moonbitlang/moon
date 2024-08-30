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

use std::sync::{Arc, Mutex};
use std::thread;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct MoonTest {
    name: String,
    status: bool,
}

impl std::fmt::Debug for MoonTest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}, {}",
            self.name,
            if self.status { "ok" } else { "failed" }
        )
    }
}

fn replace_dir(s: &str, dir: &impl AsRef<std::path::Path>) -> String {
    let path_str1 = dunce::canonicalize(dir)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let s = s.replace("\\\\", "\\");
    let s = s.replace(&path_str1, "${WORK_DIR}");
    s.replace("\r\n", "\n").replace('\\', "/")
}

#[test]
fn cmd_test() {
    // Build xtask first
    let mut build_cmd = std::process::Command::new("cargo");
    build_cmd
        .arg("build")
        .arg("--package")
        .arg("xtask")
        .arg("--locked");
    let build_status = build_cmd.status().expect("Failed to execute build command");
    assert!(build_status.success(), "Failed to build xtask");

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
            test_cases.push(entry.clone());
        }
    }

    let test_cases = Arc::new(Mutex::new(test_cases));
    let results = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();

    while let Some(test_case) = test_cases.lock().unwrap().pop() {
        let test_case = test_case.clone();
        let p = dunce::canonicalize(test_case.clone().into_path()).unwrap();
        let handle = thread::spawn(move || {
            let xtask_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("target/debug/xtask");
            let mut cmd = std::process::Command::new(xtask_path);
            cmd.arg("cmdtest").arg(test_case.into_path());
            if std::env::var("UPDATE_EXPECT").is_ok() {
                cmd.arg("-u");
            }
            let status = cmd.status().unwrap();
            let parent = p.parent().unwrap().parent().unwrap();
            MoonTest {
                name: replace_dir(p.to_str().unwrap(), &parent),
                status: status.success(),
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        let res = handle.join().unwrap();
        results.lock().unwrap().push(res);
    }

    let mut all_results = results.lock().unwrap();
    all_results.sort();
    expect_test::expect![[r#"
        [
            ${WORK_DIR}/moon_build_package.in/moon.test, ok,
            ${WORK_DIR}/moon_info_001.in/moon.test, ok,
            ${WORK_DIR}/moon_info_002.in/moon.test, ok,
            ${WORK_DIR}/specify_source_dir_001.in/moon.test, ok,
            ${WORK_DIR}/test_moon_info.in/moon.test, ok,
        ]
    "#]]
    .assert_debug_eq(&all_results);
}
