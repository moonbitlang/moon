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

//! Handles which runtime to use to run a specific output.

use std::path::Path;
use std::process::Command;

use moonbuild::entry::TestArgs;
use moonbuild_rupes_recta::model::{RunBackend, TccRunConfig};

/// Returns a command to run the given MoonBit executable of a specific
/// `backend`. The returning command is suitable for adding more commandline
/// arguments that are directly passed to the MoonBit program being executed.
///
/// If the executable is a test executable, `test` should be passed with the
/// args that are passed to the test executable. The function **may create
/// temporary files** to support test execution.
///
/// `mbt_executable` is the final MoonBit executable to run, such as a `.wasm`
/// file in WASM or WASM-GC backends, a `.js` file in JS backend, or a native
/// executable in Native or LLVM backends.
///
/// ### Note
///
pub(crate) fn command_for(
    backend: RunBackend,
    tcc_run: Option<&TccRunConfig>,
    mbt_executable: &Path,
    test: Option<&TestArgs>,
) -> Command {
    command_for_with_moonrun_policy(backend, tcc_run, mbt_executable, test, None)
}

pub(crate) fn command_for_with_moonrun_policy(
    backend: RunBackend,
    tcc_run: Option<&TccRunConfig>,
    mbt_executable: &Path,
    test: Option<&TestArgs>,
    moonrun_policy: Option<&Path>,
) -> Command {
    debug_assert!(tcc_run.is_none() || backend == RunBackend::Native);

    match (backend, tcc_run) {
        (RunBackend::Wasm | RunBackend::WasmGC, _) => {
            let mut cmd = Command::new(&*moonutil::toolchain::BINARIES.moonrun);
            if let Some(t) = test {
                cmd.arg("--test-args");
                cmd.arg(serde_json::to_string(t).unwrap());
            }
            if let Some(policy) = moonrun_policy {
                cmd.arg("--policy");
                cmd.arg(policy);
            }
            cmd.arg(mbt_executable);
            cmd.arg("--");
            cmd
        }
        (RunBackend::Js, _) => {
            if test.is_some() {
                // Also write package.json to the directory of the .js file being required
                // to prevent node from finding the user's package.json with "type": "module"
                if let Some(js_parent) = mbt_executable.parent() {
                    let js_dir_package_json = js_parent.join("package.json");
                    let _ = std::fs::write(js_dir_package_json, "{}");
                }
            }
            let mut cmd = Command::new(moonutil::toolchain::BINARIES.node_or_default());
            cmd.arg("--enable-source-maps");
            cmd.arg(mbt_executable);
            if let Some(t) = test {
                cmd.arg(serde_json::to_string(t).expect("Failed to serialize test args"));
            }
            cmd
        }
        (RunBackend::Native, Some(tcc_run)) => {
            let tcc = tcc_run.internal_tcc();
            let mut cmd = Command::new(tcc.cc_path());
            cmd.arg(format!("@{}", mbt_executable.display()));
            if let Some(t) = test {
                cmd.arg(t.to_cli_args_for_native());
            }
            cmd
        }
        (RunBackend::Native | RunBackend::Llvm, _) => {
            let mut cmd = Command::new(mbt_executable);
            if let Some(t) = test {
                cmd.arg(t.to_cli_args_for_native());
            }
            cmd
        }
    }
}
