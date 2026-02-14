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

use moonbuild::entry::TestArgs;
use moonbuild_rupes_recta::model::RunBackend;
use moonutil::compiler_flags::CC;
use tokio::process::Command;

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
/// Currently there's no support for using `tcc` to execute the target program.
pub(crate) fn command_for(
    backend: RunBackend,
    mbt_executable: &Path,
    test: Option<&TestArgs>,
) -> anyhow::Result<Command> {
    match backend {
        RunBackend::Wasm | RunBackend::WasmGC => {
            let mut cmd = Command::new(&*moonutil::BINARIES.moonrun);
            if let Some(t) = test {
                cmd.arg("--test-args");
                cmd.arg(serde_json::to_string(t).unwrap());
            }
            cmd.arg(mbt_executable);
            cmd.arg("--");
            Ok(cmd)
        }
        RunBackend::Js => {
            if test.is_some() {
                // Also write package.json to the directory of the .js file being required
                // to prevent node from finding the user's package.json with "type": "module"
                if let Some(js_parent) = mbt_executable.parent() {
                    let js_dir_package_json = js_parent.join("package.json");
                    let _ = std::fs::write(js_dir_package_json, "{}");
                }
            }
            let mut cmd = Command::new(moonutil::BINARIES.node_or_default());
            cmd.arg("--enable-source-maps");
            cmd.arg(mbt_executable);
            if let Some(t) = test {
                cmd.arg(serde_json::to_string(t).expect("Failed to serialize test args"));
            }
            Ok(cmd)
        }
        RunBackend::Native | RunBackend::Llvm => {
            let mut cmd = Command::new(mbt_executable);
            if let Some(t) = test {
                cmd.arg(t.to_cli_args_for_native());
            }
            Ok(cmd)
        }
        RunBackend::NativeTccRun => {
            let tcc = CC::internal_tcc().expect("TCC must be available for TCC run backend");
            let mut cmd = Command::new(tcc.cc_path());
            cmd.arg(format!("@{}", mbt_executable.display()));
            if let Some(t) = test {
                cmd.arg(t.to_cli_args_for_native());
            }
            Ok(cmd)
        }
    }
}
