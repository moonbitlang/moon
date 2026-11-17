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

//! Runtime-facing `moonbitlang/core` imports for the linear wasm backend.

mod environment;
mod registry;
mod windows_time;

pub(crate) use registry::register;

use std::{ffi::OsStr, path::Path};

use crate::async_host::AsyncHostResult;
use crate::runtime::Runtime;

pub(crate) const MOONBIT_CORE_MODULE: &str = "moonbitlang/core";

pub(crate) fn current_exe(runtime: &Runtime) -> u64 {
    buffer_to_handle(
        runtime,
        runtime
            .executable()
            .path()
            .map(path_to_native_string_buffer),
    )
}

pub(crate) fn current_exe_dir(runtime: &Runtime) -> u64 {
    buffer_to_handle(
        runtime,
        runtime
            .executable()
            .directory()
            .map(path_to_native_string_buffer),
    )
}

fn buffer_to_handle(runtime: &Runtime, buffer: AsyncHostResult<Box<[u8]>>) -> u64 {
    match buffer {
        Ok(buffer) => runtime.async_host().insert_c_buffer(buffer),
        Err(error) => {
            runtime.async_host().record_error(error);
            runtime.null_handle()
        }
    }
}

fn path_to_native_string_buffer(path: &Path) -> Box<[u8]> {
    // Preserve the existing executable-path ABI's native NUL terminator.
    let mut bytes = native_string_bytes(path.as_os_str());
    bytes.push(0);
    #[cfg(windows)]
    bytes.push(0);
    bytes.into_boxed_slice()
}

#[cfg(unix)]
fn native_string_bytes(value: &OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    value.as_bytes().to_vec()
}

#[cfg(windows)]
fn native_string_bytes(value: &OsStr) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    value.encode_wide().flat_map(u16::to_le_bytes).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_host::AsyncHostError;
    use crate::runtime::Executable;

    #[test]
    fn in_memory_module_cannot_produce_current_exe() {
        let runtime = Runtime::new(
            crate::runtime::RuntimeConfig::default(),
            Executable::unavailable(),
            crate::signal_channel().1,
        )
        .unwrap();

        for import in [current_exe, current_exe_dir] {
            runtime.async_host().set_errno(0);
            assert_eq!(import(&runtime), runtime.null_handle());
            assert_eq!(
                runtime.async_host().get_errno(),
                AsyncHostError::Inval.errno()
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn native_string_buffer_preserves_wide_units_and_terminates() {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;

        let path = std::path::PathBuf::from(OsString::from_wide(&[b'a' as u16, 0xd800]));
        assert_eq!(
            path_to_native_string_buffer(&path).as_ref(),
            &[b'a', 0, 0, 0xd8, 0, 0]
        );
    }

    #[cfg(unix)]
    #[test]
    fn native_string_buffer_preserves_non_utf8_bytes_and_terminates() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let path = std::path::PathBuf::from(OsString::from_vec(vec![b'/', 0xff]));
        let executable = Executable::from_file(&path);
        assert_eq!(
            path_to_native_string_buffer(executable.path().unwrap()).as_ref(),
            &[b'/', 0xff, 0]
        );
        assert_eq!(
            path_to_native_string_buffer(executable.directory().unwrap()).as_ref(),
            &[b'/', 0]
        );
    }
}
