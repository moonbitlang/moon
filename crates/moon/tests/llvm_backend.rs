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
            r#"#cfg(target="native")
pub fn answer() -> Int { from_manifest() + from_filename() }
pub fn require_native_answer() -> Int { answer() }
"#,
        ),
    ] {
        std::fs::write(project.path().join(name), contents).unwrap();
    }

    // The compiler enables native declarations for LLVM too, so file selection
    // must retain their implementations. The unconditional caller ensures
    // disabling the native cfg cannot make this test pass vacuously.
    for backend in ["native", "llvm"] {
        snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
            .current_dir(project.path())
            .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
            .env("MOON_DEP_CACHE", "off")
            .env("MOON_WORK", "off")
            .args(["check", "--target", backend])
            .assert()
            .success()
            .stdout_eq("");
    }
}

#[test]
fn missing_llvm_bundle_is_built_from_source() {
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
    std::fs::write(
        core.path().join("prelude/moon.pkg"),
        "import { \"moonbitlang/core/builtin\" }\n",
    )
    .unwrap();
    std::fs::write(
        core.path().join("prelude/prelude.mbt"),
        "pub using @builtin {type Int, type Unit}\n",
    )
    .unwrap();
    std::fs::create_dir(core.path().join("builtin")).unwrap();
    std::fs::write(core.path().join("builtin/moon.pkg"), "").unwrap();
    std::fs::write(
        core.path().join("builtin/builtin.mbt"),
        "#builtin\npub type Int\n#builtin\npub type Unit\n",
    )
    .unwrap();

    snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
        .current_dir(project.path())
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .env("MOON_CORE_OVERRIDE", core.path())
        .env("MOON_DEP_CACHE", "off")
        .env("MOON_WORK", "off")
        .args(["check", "--target", "llvm"])
        .assert()
        .success()
        .stdout_eq("");
    for package in ["builtin", "prelude"] {
        assert!(
            project
                .path()
                .join(format!(
                    "_build/llvm/debug/check/.mooncakes/moonbitlang/core/{package}/{package}.mi"
                ))
                .is_file()
        );
    }
    assert!(!core.path().join("_build").exists());
}
