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

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

struct Tools {
    moon: PathBuf,
    moonrun: PathBuf,
    toolchain_root: PathBuf,
}

fn tools() -> &'static Tools {
    static TOOLS: OnceLock<Tools> = OnceLock::new();
    TOOLS.get_or_init(|| {
        // A separate test crate has no CARGO_BIN_EXE_moon. Ask Cargo for both
        // host tools once, using the workspace manifest and default target
        // layout so features and artifacts match the normal workspace build.
        let messages = escargot::CargoBuild::new()
            .manifest_path(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml"))
            .bin("moon")
            .bin("moonrun")
            .current_release()
            .arg("--locked")
            .exec()
            .expect("build Moon E2E tools");
        let mut moon = None;
        let mut moonrun = None;
        // Consume the entire stream so Cargo's final exit status is checked,
        // even if it reported both executables before a later build failure.
        for message in messages {
            let message = message.expect("build Moon E2E tools");
            match message.decode().expect("decode Cargo build output") {
                escargot::format::Message::CompilerArtifact(artifact)
                    if artifact.target.kind == ["bin"] && !artifact.profile.test =>
                {
                    match artifact.target.name.as_ref() {
                        "moon" => moon = artifact.executable.map(|path| path.into_owned()),
                        "moonrun" => moonrun = artifact.executable.map(|path| path.into_owned()),
                        _ => {}
                    }
                }
                escargot::format::Message::CompilerMessage(message) => {
                    if let Some(rendered) = message.message.rendered {
                        eprint!("{rendered}");
                    }
                }
                _ => {}
            }
        }

        // Match the compiler selected from PATH, including installations where
        // the compiler's toolchain differs from the parent process's default.
        let toolchain_root = std::env::var_os("MOON_TOOLCHAIN_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let moonc = dunce::canonicalize(&*moonutil::toolchain::BINARIES.moonc)
                    .unwrap_or(moonutil::toolchain::BINARIES.moonc.clone());
                if let Some(bin_dir) = moonc.parent()
                    && bin_dir.file_name().is_some_and(|name| name == "bin")
                    && let Some(root) = bin_dir.parent()
                    && moonutil::toolchain::is_toolchain_root(root)
                {
                    root.to_path_buf()
                } else {
                    moonutil::toolchain::toolchain_root()
                }
            });
        Tools {
            moon: moon.expect("Cargo reported the moon executable"),
            moonrun: moonrun.expect("Cargo reported the moonrun executable"),
            toolchain_root,
        }
    })
}

pub(super) fn moon_cmd(project: &Path) -> snapbox::cmd::Command {
    command(&tools().moon, project).env("MOONRUN_OVERRIDE", &tools().moonrun)
}

pub(super) fn moonrun_cmd(project: &Path) -> snapbox::cmd::Command {
    command(&tools().moonrun, project)
}

fn command(executable: &Path, project: &Path) -> snapbox::cmd::Command {
    let tools = tools();
    let mut redactions = snapbox::Redactions::new();
    redactions.insert("[WORK_DIR]", project.to_owned()).unwrap();
    let canonical_project = std::fs::canonicalize(project).expect("project exists");
    // Compiler diagnostics can omit the Windows verbatim prefix. Keep that
    // spelling alongside the original and fully canonical paths.
    redactions
        .insert(
            "[WORK_DIR]",
            dunce::simplified(&canonical_project).to_owned(),
        )
        .unwrap();
    redactions.insert("[WORK_DIR]", canonical_project).unwrap();
    redactions
        .insert("[TOOLCHAIN]", tools.toolchain_root.clone())
        .unwrap();
    // Dry-run output can spell the same root using Moon's path placeholders.
    redactions.insert("[TOOLCHAIN]", "$MOON_HOME").unwrap();
    redactions
        .insert("[TOOLCHAIN]", "$MOON_TOOLCHAIN_ROOT")
        .unwrap();

    snapbox::cmd::Command::new(executable)
        .current_dir(project)
        .with_assert(snapbox::Assert::new().redact_with(redactions))
        .env("MOON_TOOLCHAIN_ROOT", &tools.toolchain_root)
        .env("MOON_DEP_CACHE", "off")
}
