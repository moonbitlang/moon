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
use moonutil::common::TargetBackend;
use tokio::process::Command;

macro_rules! cache {
    ($(
        $id:ident(
            $first_candidate:expr
            $(,$candidate:expr)* $(,)?
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

/// Returns a command to run the given MoonBit executable of a specific
/// `backend`. The returning command is suitable for adding more commandline
/// arguments that are directly passed to the MoonBit program being executed.
///
/// If the executable is a test executable, `test` should be passed with the
/// args that are passed to the test executable. (TBD)
///
/// `mbt_executable` is the final MoonBit executable to run, such as a `.wasm`
/// file in WASM or WASM-GC backends, a `.js` file in JS backend, or a native
/// executable in Native or LLVM backends.
///
/// ### Note
///
/// Currently there's no support for using `tcc` to execute the target program.
pub fn command_for(
    backend: TargetBackend,
    mbt_executable: &Path,
    test: Option<TestArgs>,
) -> Command {
    let cache = RuntimeExecutableCache::default();
    command_for_cached(&cache, backend, mbt_executable, test)
}

pub fn command_for_cached(
    cache: &RuntimeExecutableCache,
    backend: TargetBackend,
    mbt_executable: &Path,
    test: Option<TestArgs>,
) -> Command {
    if test.is_some() {
        todo!("Test execution is not yet implemented");
    }
    match backend {
        TargetBackend::Wasm | TargetBackend::WasmGC => {
            let mut cmd = Command::new(cache.moonrun());
            cmd.arg(mbt_executable);
            cmd.arg("--");
            cmd
        }
        TargetBackend::Js => {
            // js test needs a custom driver
            let mut cmd = Command::new(cache.node());
            cmd.arg(mbt_executable);
            cmd.arg("--");
            cmd
        }
        TargetBackend::Native | TargetBackend::LLVM => Command::new(mbt_executable),
    }
}
