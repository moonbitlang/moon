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

//! Handles which runtime to use to run a specific output.

use std::path::Path;
use std::process::Command;

use moonbuild::entry::TestArgs;
use moonbuild_rupes_recta::model::BackendConfig;

/// The runtime mode used to execute one built artifact.
#[derive(Clone, Copy)]
pub(crate) enum ExecutionMode {
    MoonRun,
    Node,
    Native,
}

impl From<&BackendConfig> for ExecutionMode {
    fn from(backend: &BackendConfig) -> Self {
        match backend {
            BackendConfig::Wasm { .. } | BackendConfig::WasmGc { .. } => Self::MoonRun,
            BackendConfig::Js => Self::Node,
            BackendConfig::Native { .. } | BackendConfig::Llvm { .. } => Self::Native,
        }
    }
}

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
    mode: ExecutionMode,
    mbt_executable: &Path,
    test: Option<&TestArgs>,
) -> Command {
    command_for_with_moonrun_policy(mode, mbt_executable, test, None)
}

pub(crate) fn command_for_with_moonrun_policy(
    mode: ExecutionMode,
    mbt_executable: &Path,
    test: Option<&TestArgs>,
    moonrun_policy: Option<&Path>,
) -> Command {
    command_for_with_moonrun_policy_source_dir(mode, mbt_executable, test, moonrun_policy, None)
}

pub(crate) fn command_for_with_moonrun_policy_source_dir(
    mode: ExecutionMode,
    mbt_executable: &Path,
    test: Option<&TestArgs>,
    moonrun_policy: Option<&Path>,
    policy_source_dir: Option<&Path>,
) -> Command {
    match mode {
        ExecutionMode::MoonRun => {
            let mut cmd = Command::new(&*moonutil::toolchain::BINARIES.moonrun);
            if let Some(t) = test {
                cmd.arg("--test-args");
                cmd.arg(serde_json::to_string(t).unwrap());
            }
            if let Some(policy) = moonrun_policy {
                cmd.arg("--policy");
                cmd.arg(policy);
            }
            if let Some(source_dir) = policy_source_dir {
                debug_assert!(moonrun_policy.is_some());
                cmd.arg("--policy-source-dir");
                cmd.arg(source_dir);
            }
            cmd.arg(mbt_executable);
            cmd.arg("--");
            cmd
        }
        ExecutionMode::Node => {
            let mut cmd = Command::new(moonutil::toolchain::BINARIES.node_or_default());
            cmd.arg("--enable-source-maps");
            cmd.arg(mbt_executable);
            if let Some(t) = test {
                cmd.arg(serde_json::to_string(t).expect("Failed to serialize test args"));
            }
            cmd
        }
        ExecutionMode::Native => {
            let mut cmd = Command::new(mbt_executable);
            if let Some(t) = test {
                cmd.arg(t.to_cli_args_for_native());
            }
            cmd
        }
    }
}
