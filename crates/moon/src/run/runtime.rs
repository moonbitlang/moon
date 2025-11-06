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

use std::{
    cell::OnceCell,
    path::{Path, PathBuf},
};

use moonbuild::entry::TestArgs;
use moonbuild_rupes_recta::model::RunBackend;
use moonutil::compiler_flags::CC;
use tempfile::TempDir;
use tokio::process::Command;

macro_rules! cache {
    ($(
        $id:ident(
            $first_candidate:expr_2021
            $(,$candidate:expr_2021)* $(,)?
        )
    ),*$(,)?) => {
        /// A non-global cache for finding executables to use in compilation
        #[derive(Default)]
        pub struct RuntimeExecutableCache {
            $(
                $id: OnceCell<PathBuf>
            ),*
        }

        impl RuntimeExecutableCache {
            $(
                pub fn $id(&self) -> &Path {
                    self.$id.get_or_init(|| {
                        which::which($first_candidate)
                        $(.or_else(|_| which::which($candidate)))*
                        .unwrap_or($first_candidate.into())
                    })
                }
            )*
        }
    };
}

cache! {
    node("node", "node.cmd"),
    moonrun("moonrun")
}

/// A guarded command info that removes the temporary file/dir(s) when it gets
/// out of scope.
pub struct CommandGuard {
    _temp_file: Option<TempDir>, // for destructor
    pub command: Command,
}

impl From<Command> for CommandGuard {
    fn from(command: Command) -> Self {
        Self {
            _temp_file: None,
            command,
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
/// Currently there's no support for using `tcc` to execute the target program.
pub fn command_for(
    backend: RunBackend,
    mbt_executable: &Path,
    test: Option<&TestArgs>,
) -> anyhow::Result<CommandGuard> {
    let cache = RuntimeExecutableCache::default();
    command_for_cached(&cache, backend, mbt_executable, test)
}

pub fn command_for_cached(
    cache: &RuntimeExecutableCache,
    backend: RunBackend,
    mbt_executable: &Path,
    test: Option<&TestArgs>,
) -> anyhow::Result<CommandGuard> {
    match backend {
        RunBackend::Wasm | RunBackend::WasmGC => {
            let mut cmd = Command::new(cache.moonrun());
            if let Some(t) = test {
                cmd.arg("--test-args");
                cmd.arg(serde_json::to_string(t).unwrap());
            }
            cmd.arg(mbt_executable);
            cmd.arg("--");
            Ok(cmd.into())
        }
        RunBackend::Js => {
            if let Some(t) = test {
                let (dir, driver) = create_js_driver(mbt_executable, t)?;
                let mut cmd = Command::new(cache.node());
                cmd.arg("--enable-source-maps");
                cmd.arg(driver);
                cmd.arg(serde_json::to_string(t).expect("Failed to serialize test args"));
                Ok(CommandGuard {
                    _temp_file: Some(dir),
                    command: cmd,
                })
            } else {
                let mut cmd = Command::new(cache.node());
                cmd.arg(mbt_executable);
                Ok(cmd.into())
            }
        }
        RunBackend::Native | RunBackend::Llvm => {
            let mut cmd = Command::new(mbt_executable);
            if let Some(t) = test {
                cmd.arg(t.to_cli_args_for_native());
            }
            Ok(cmd.into())
        }
        RunBackend::NativeTccRun => {
            let tcc = CC::internal_tcc().expect("TCC must be available for TCC run backend");
            let mut cmd = Command::new(tcc.cc_path());
            cmd.arg(format!("@{}", mbt_executable.display()));
            if let Some(t) = test {
                cmd.arg(t.to_cli_args_for_native());
            }
            Ok(cmd.into())
        }
    }
}

fn create_js_driver(js_path: &Path, test_args: &TestArgs) -> anyhow::Result<(TempDir, PathBuf)> {
    let js_driver_text = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../moonbuild/template/test_driver/js_driver.js"
    ));

    // This replicates the original behavior, needs fixing later
    let js_driver = js_driver_text
        .replace(
            "origin_js_path",
            &js_path.display().to_string().replace("\\", "/"),
        )
        .replace(
            "let testParams = []",
            &format!("let testParams = {}", test_args.to_args()),
        )
        .replace(
            "let packageName = \"\"",
            &format!("let packageName = {:?}", test_args.package),
        );

    let dir = TempDir::new().expect("Failed to create temporary directory for JS testing script");
    let js_file = dir.path().join("driver.cjs");
    std::fs::write(&js_file, js_driver).expect("Failed to write temporary JS test driver script");

    // prevent node use the outer layer package.json with `"type": "module"`
    let package_json = dir.path().join("package.json");
    std::fs::write(package_json, "{}")
        .expect("Failed to write temporary package.json for JS testing script");

    Ok((dir, js_file))
}
