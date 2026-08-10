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

use moon_test_util::test_dir::TestDir;

fn test_dir(case: &str) -> TestDir {
    let case_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases");
    TestDir::from_case_root(case_root, format!("package_prebuild_plan/{case}.in"), true)
}

fn moon_check(dir: &TestDir) -> snapbox::cmd::Command {
    snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
        .args(["check", "--target", "wasm-gc"])
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .env("MOON_DEP_CACHE", "off")
        .current_dir(dir)
}

#[test]
fn unconsumed_package_prebuild_output_remains_an_execution_root() {
    let dir = test_dir("unconsumed_output");

    assert!(!dir.join("src/main/generated.txt").exists());
    moon_check(&dir).assert().success();
    assert!(dir.join("src/main/generated.txt").exists());
}
