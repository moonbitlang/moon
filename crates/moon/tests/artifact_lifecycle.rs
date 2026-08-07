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

fn dry_run(
    dir: &moon_test_util::test_dir::TestDir,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let output = snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
        .args(args)
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .env("MOON_DEP_CACHE", "off")
        .current_dir(dir)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(output).expect("moon dry-run output should be UTF-8")
}

#[test]
fn virtual_contract_uses_lifecycle_interface_dependencies() {
    let case_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases");
    let dir = moon_test_util::test_dir::TestDir::from_case_root(
        case_root,
        "virtual_pkg_test/virtual_artifact_lifecycle.in",
        true,
    );

    let build = dry_run(
        &dir,
        ["build", "src/virtual", "--target", "wasm-gc", "--dry-run"],
    );
    let build_interface = build
        .lines()
        .find(|line| line.contains("moonc build-interface"))
        .expect("build graph should compile the virtual contract");
    assert!(build_interface.contains("/debug/build/dep/dep.mi"));
    assert!(!build_interface.contains("/debug/check/dep/dep.mi"));

    let check = dry_run(
        &dir,
        ["check", "src/virtual", "--target", "wasm-gc", "--dry-run"],
    );
    let check_interface = check
        .lines()
        .find(|line| line.contains("moonc build-interface"))
        .expect("check graph should compile the virtual contract");
    assert!(check_interface.contains("/debug/check/dep/dep.mi"));
    assert!(!check_interface.contains("/debug/build/dep/dep.mi"));
}
