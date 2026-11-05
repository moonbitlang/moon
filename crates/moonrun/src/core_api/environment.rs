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

//! Native-string buffer adaptation for the Runtime's cwd and environment.
//! See `docs/dev/core-environment-abi.md` for the guest contract.

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::guest_memory::GuestMemory;
use crate::runtime::{EnvError, Runtime};
use std::{borrow::Cow, ffi::OsStr};

#[cfg(unix)]
mod native_error {
    pub(super) const MISSING_VARIABLE: i32 = libc::ENOENT;
    pub(super) const LENGTH_OVERFLOW: i32 = libc::EOVERFLOW;
}

#[cfg(windows)]
mod native_error {
    use windows_sys::Win32::Foundation::{ERROR_ARITHMETIC_OVERFLOW, ERROR_ENVVAR_NOT_FOUND};
    pub(super) const MISSING_VARIABLE: i32 = ERROR_ENVVAR_NOT_FOUND as i32;
    pub(super) const LENGTH_OVERFLOW: i32 = ERROR_ARITHMETIC_OVERFLOW as i32;
}

pub(super) fn current_dir(runtime: &Runtime) -> AsyncHostResult<Box<[u8]>> {
    let path = runtime.filesystem().current_directory()?;
    snapshot_buffer(path.as_os_str())
}

pub(super) fn get_env_var(
    runtime: &Runtime,
    memory: &[u8],
    key_ptr: u32,
    key_len: u32,
) -> AsyncHostResult<Box<[u8]>> {
    let key = read_native_string(memory, key_ptr, key_len)?;
    let value = runtime
        .environment()
        .get(key.as_ref())
        .ok_or(AsyncHostError::Native(native_error::MISSING_VARIABLE))?;
    snapshot_buffer(&value)
}

pub(super) fn set_env_var(
    runtime: &Runtime,
    memory: &[u8],
    key_ptr: u32,
    key_len: u32,
    value_ptr: u32,
    value_len: u32,
) -> AsyncHostResult<()> {
    // Decode both inputs before mutation. Env owns name/NUL validation
    // and ambient-versus-owned behavior for every import family.
    let key = read_native_string(memory, key_ptr, key_len)?;
    let value = read_native_string(memory, value_ptr, value_len)?;
    runtime
        .environment()
        .set(key.into_owned(), value.into_owned())
        .map_err(env_error)
}

pub(super) fn unset_env_var(
    runtime: &Runtime,
    memory: &[u8],
    key_ptr: u32,
    key_len: u32,
) -> AsyncHostResult<()> {
    let key = read_native_string(memory, key_ptr, key_len)?;
    runtime.environment().unset(key.as_ref()).map_err(env_error)
}

fn read_native_string(memory: &[u8], offset: u32, length: u32) -> AsyncHostResult<Cow<'_, OsStr>> {
    let bytes = memory.read_exact(offset, length)?;
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(Cow::Borrowed(OsStr::from_bytes(bytes)))
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        // Length is in bytes. Decode unaligned little-endian code units without
        // Unicode validation: Windows strings may contain unpaired surrogates.
        if !bytes.len().is_multiple_of(2) {
            return Err(AsyncHostError::Inval);
        }
        let units = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        Ok(Cow::Owned(std::ffi::OsString::from_wide(&units)))
    }
}

fn snapshot_buffer(value: &OsStr) -> AsyncHostResult<Box<[u8]>> {
    let bytes = super::native_string_bytes(value);
    // CBuffer's length/copy ABI uses u32 byte counts. Publish an owned snapshot,
    // so later copies never query Env/cwd again or retain a Guest Memory borrow.
    u32::try_from(bytes.len())
        .map_err(|_| AsyncHostError::Native(native_error::LENGTH_OVERFLOW))?;
    Ok(bytes.into_boxed_slice())
}

fn env_error(error: EnvError) -> AsyncHostError {
    match error {
        EnvError::InvalidName(_) | EnvError::ValueContainsNul(_) => AsyncHostError::Inval,
        EnvError::Set { source, .. } | EnvError::Unset { source, .. } => source.into(),
    }
}
