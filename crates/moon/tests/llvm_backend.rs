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

#[test]
fn native_file_conditions_match_compiler_cfg_on_llvm() {
    let project = tempfile::tempdir().unwrap();
    for (name, contents) in [
        ("moon.mod", "name = \"test/llvm\"\n"),
        (
            "moon.pkg",
            "options(targets: { \"provider.mbt\": [\"native\"] })\n",
        ),
        ("provider.mbt", "pub fn from_manifest() -> Int { 1 }\n"),
        (
            "provider.native.mbt",
            "pub fn from_filename() -> Int { 2 }\n",
        ),
        (
            "consumer.mbt",
            "#cfg(target=\"native\")\npub fn answer() -> Int { from_manifest() + from_filename() }\n",
        ),
    ] {
        std::fs::write(project.path().join(name), contents).unwrap();
    }

    // The compiler enables native declarations for LLVM too, so file selection
    // must retain their implementations without requiring an installed stdlib.
    for backend in ["native", "llvm"] {
        snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
            .current_dir(project.path())
            .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
            .env("MOON_DEP_CACHE", "off")
            .env("MOON_WORK", "off")
            .args(["check", "--nostd", "--target", backend])
            .assert()
            .success()
            .stdout_eq("");
    }
}

#[test]
fn missing_llvm_prelude_reports_an_actionable_error() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("moon.mod"), "name = \"test/llvm\"\n").unwrap();
    std::fs::write(project.path().join("moon.pkg"), "").unwrap();
    std::fs::write(
        project.path().join("lib.mbt"),
        "pub fn answer() -> Int { 42 }\n",
    )
    .unwrap();

    // A core checkout without a compiled LLVM bundle.
    let core = tempfile::tempdir().unwrap();
    std::fs::write(
        core.path().join("moon.mod"),
        "name = \"moonbitlang/core\"\n",
    )
    .unwrap();
    std::fs::create_dir(core.path().join("prelude")).unwrap();
    std::fs::write(core.path().join("prelude/moon.pkg"), "").unwrap();

    snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
        .current_dir(project.path())
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .env("MOON_CORE_OVERRIDE", core.path())
        .env("MOONC_OVERRIDE", project.path().join("missing-moonc"))
        .env("MOON_DEP_CACHE", "off")
        .env("MOON_WORK", "off")
        .args(["check", "--target", "llvm"])
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
Warning: LLVM backend is experimental and only supported on nightly moonbit toolchain for now
Error: failed to run check for target LLVM

Caused by:
    0: Failed to calculate build plan
    1: toolchain is incomplete: missing `prelude.mi` for target `llvm`

"#]]);
}
