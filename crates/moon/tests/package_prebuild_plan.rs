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

fn moon_test(dir: &TestDir) -> snapbox::cmd::Command {
    snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
        .args(["test", "--target", "wasm-gc"])
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

#[test]
fn generated_mbtp_is_a_check_input() {
    let dir = test_dir("generated_mbtp");

    moon_check(&dir)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
moon tool exec --shell '[..]moon[EXE] tool embed --text -i ./src/main/proof.txt -o ./src/main/generated.mbtp --name ignored'
  cwd: .
moonc check ./src/main/main.mbt ./src/main/generated.mbtp [..]
[..]

"#]]);
}

#[test]
fn generated_moonlex_input_forms_a_prebuild_pipeline() {
    let dir = test_dir("generated_moonlex_input");

    moon_check(&dir)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
moon tool exec --shell '[..]moon[EXE] tool embed --text -i ./src/main/lexer.txt -o ./src/main/generated.mbl --name ignored'
  cwd: .
moonrun [..]moonlex[..] -- ./src/main/generated.mbl -o ./src/main/generated.mbt
moonc check ./src/main/generated.mbt ./src/main/main.mbt [..]
[..]

"#]]);
}

#[test]
fn generated_mbt_md_is_a_blackbox_test_input() {
    let dir = test_dir("generated_mbt_md");

    moon_test(&dir)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
[..]
[..]
[..]
[..]
moon tool exec --shell '[..]moon[EXE] tool embed --text -i ./src/lib/guide.txt -o ./src/lib/generated.mbt.md --name ignored'
  cwd: .
moon generate-test-driver [..] ./src/lib/generated.mbt.md [..]
moonc build-package ./src/lib/generated.mbt.md [..]
[..]

"#]]);
}

#[test]
fn generated_moonyacc_input_forms_a_prebuild_pipeline() {
    let dir = test_dir("generated_moonyacc_input");

    moon_check(&dir)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
moon tool exec --shell '[..]moon[EXE] tool embed --text -i ./src/main/parser.txt -o ./src/main/generated.mby --name ignored'
  cwd: .
moonrun [..]moonyacc[..] -- ./src/main/generated.mby -o ./src/main/generated.mbt
moonc check ./src/main/generated.mbt ./src/main/main.mbt [..]
[..]

"#]]);
}

#[test]
fn generated_mbt_md_warns_for_main_package() {
    let dir = test_dir("generated_main_mbt_md");

    moon_test(&dir)
        .arg("--dry-run")
        .assert()
        .success()
        .stderr_eq(snapbox::str![[r#"
[..]Warning: Main package `username/generated_main_mbt_md/main` uses blackbox-only test inputs (`.mbt.md` files) [..]

"#]]);
}
