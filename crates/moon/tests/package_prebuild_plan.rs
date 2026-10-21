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
fn multi_backend_check_shares_the_package_prebuild_provider() {
    let dir = test_dir("unconsumed_output");

    assert!(!dir.join("src/main/generated.txt").exists());
    snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
        .args(["check", "--target", "all"])
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .env("MOON_DEP_CACHE", "off")
        .current_dir(&dir)
        .assert()
        .success();
    assert!(dir.join("src/main/generated.txt").exists());
}

#[test]
fn builtin_embed_handles_moon_path_with_spaces() {
    let dir = test_dir("unconsumed_output");
    let moon_bin_dir = dir.join("moon bin");
    std::fs::create_dir(&moon_bin_dir).expect("failed to create moon bin directory");
    let moon = moon_bin_dir.join(format!("moon{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(snapbox::cargo_bin!("moon"), &moon).expect("failed to copy moon executable");

    assert!(!dir.join("src/main/generated.txt").exists());
    snapbox::cmd::Command::new(&moon)
        .args(["check", "--target", "wasm-gc"])
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .env("MOON_DEP_CACHE", "off")
        .current_dir(&dir)
        .assert()
        .success();
    assert!(dir.join("src/main/generated.txt").exists());
}

#[test]
fn builtin_embed_rejects_shell_syntax() {
    let dir = test_dir("unconsumed_output");
    let manifest = dir.join("src/main/moon.pkg");
    let original = std::fs::read_to_string(&manifest).expect("failed to read package manifest");
    let modified = original.replace(
        ":embed --text -i $input -o $output --name ignored",
        r#":embed --text -i $input -o $output --name \"$(touch shell-substitution; printf ignored)\" && touch shell-list"#,
    );
    assert_ne!(modified, original, "failed to modify prebuild command");
    std::fs::write(manifest, modified).expect("failed to write package manifest");

    moon_check(&dir)
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: failed to run script username/unconsumed_prebuild/main generated.txt (prebuild): [..] tool embed [..]
error: unexpected argument '&&' found
Usage: moon[..] tool embed [OPTIONS] --input <INPUT> --output <OUTPUT>
For more information, try '--help'.
Error: failed when checking project

"#]]);
    assert!(!dir.join("src/main/generated.txt").exists());
    assert!(!dir.join("shell-substitution").exists());
    assert!(!dir.join("shell-list").exists());
}

#[cfg(not(target_os = "windows"))]
#[test]
fn builtin_embed_rejects_malformed_quoting() {
    let dir = test_dir("unconsumed_output");
    let manifest = dir.join("src/main/moon.pkg");
    let original = std::fs::read_to_string(&manifest).expect("failed to read package manifest");
    let modified = original.replace(
        ":embed --text -i $input -o $output --name ignored",
        r#":embed --text -i $input -o $output --name ignored \"unterminated"#,
    );
    assert_ne!(modified, original, "failed to modify prebuild command");
    std::fs::write(manifest, modified).expect("failed to write package manifest");

    moon_check(&dir)
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: failed to run check for target WasmGC

Caused by:
    0: Failed to calculate build plan
    1: Failed to lower the build plan
    2: failed to parse built-in :embed prebuild arguments: unterminated quote or escape

"#]]);
    assert!(!dir.join("src/main/generated.txt").exists());
}

#[test]
fn generated_mbtp_is_a_check_input() {
    let dir = test_dir("generated_mbtp");

    moon_check(&dir)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
moon tool embed --text -i ./src/main/proof.txt -o ./src/main/generated.mbtp --name ignored
  cwd: .
moonc check ./src/main/main.mbt ./src/main/generated.mbtp [..]
[..]

"#]]);
}

#[test]
fn generated_mbti_supplies_a_virtual_package_contract() {
    let dir = test_dir("generated_virtual_mbti");

    moon_check(&dir)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
moon tool embed --text -i ./src/virtual/contract.txt -o ./src/virtual/pkg.mbti --name ignored
  cwd: .
moonc build-interface ./src/virtual/pkg.mbti [..]

"#]]);
}

#[test]
fn legacy_outputs_remain_ordinary_custom_prebuild_outputs() {
    for (case, extension) in [
        ("generated_moonlex_input", "mbl"),
        ("generated_moonyacc_input", "mby"),
    ] {
        let dir = test_dir(case);
        for _ in 0..2 {
            moon_check(&dir).assert().success();
            assert!(dir.join(format!("src/main/generated.{extension}")).exists());
            assert!(!dir.join("src/main/generated.mbt").exists());
        }
        // An explicit second rule can consume the output without an implicit generator.
        let manifest = dir.join("src/main/moon.pkg");
        let contents = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(
            &manifest,
            format!(
                r#"{contents}
dev_build(rule: "generate", input: "generated.{extension}", output: "generated.mbt")
"#
            ),
        )
        .unwrap();
        moon_check(&dir).assert().success();
        assert!(dir.join("src/main/generated.mbt").exists());
    }
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
moon tool embed --text -i ./src/lib/guide.txt -o ./src/lib/generated.mbt.md --name ignored
  cwd: .
moon generate-test-driver [..] ./src/lib/generated.mbt.md [..]
moonc build-package ./src/lib/generated.mbt.md [..]
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

#[test]
fn implicit_legacy_generators_report_migration() {
    for extension in ["mbl", "mby"] {
        let dir = test_dir("unconsumed_output");
        std::fs::write(
            dir.join(format!("src/main/legacy.{extension}")),
            "legacy input",
        )
        .unwrap();
        moon_check(&dir)
            .arg("--dry-run")
            .assert()
            .failure()
            .stderr_eq(snapbox::str![[r#"
...
[..]Automatic generation from '[..]' is no longer supported. Declare the file in a custom `rule` / `dev_build` (or `pre-build`) task that generates `.mbt` files.
...
"#]]);
    }
}

#[test]
fn explicit_legacy_inputs_use_only_custom_prebuild() {
    for extension in ["mbl", "mby"] {
        let dir = test_dir("unconsumed_output");
        std::fs::write(
            dir.join(format!("src/main/legacy.{extension}")),
            "custom input",
        )
        .unwrap();
        let input = dir.join(format!("src/main/legacy.{extension}"));
        let absolute = dunce::canonicalize(input).unwrap();
        for declared in [
            format!("./legacy.{extension}"),
            format!("../main/legacy.{extension}"),
            absolute.to_str().unwrap().to_owned(),
        ] {
            std::fs::write(
                dir.join("src/main/moon.pkg"),
                format!(
                    r#"
pkgtype(kind: "executable")
rule(name: "generate", command: ":embed --text -i $input -o $output --name _legacy_text")
dev_build(rule: "generate", input: {declared:?}, output: "generated.mbt")
"#
                ),
            )
            .unwrap();
            for _ in 0..2 {
                moon_check(&dir).assert().success();
                assert!(dir.join("src/main/generated.mbt").exists());
                assert!(!dir.join("src/main/legacy.mbt").exists());
            }
        }
    }
}
