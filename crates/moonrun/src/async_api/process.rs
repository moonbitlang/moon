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

#[cfg(target_os = "linux")]
use crate::async_host::AsyncHostError;
use crate::async_host::AsyncHostResult;
#[cfg(windows)]
use crate::async_host::read_u16;
use crate::guest_memory::GuestMemory;

use super::context::ImportContext;
use super::os_string::read_guest as read_guest_os_string;
use super::provenance::ported_imports;

ported_imports! {
#[ported(source = "src/process/unix.c", original = "moonbitlang_async_get_curr_env")]
#[cfg(unix)]
pub(super) fn get_curr_env(context: &mut ImportContext<'_>) -> u64 {
    context.host.process_inputs().capture_legacy_env()
}

#[ported(source = "src/process/windows.c", original = "moonbitlang_async_get_curr_env")]
#[cfg(windows)]
pub(super) fn get_curr_env(context: &mut ImportContext<'_>) -> AsyncHostResult<u64> {
    Ok(context.host.process_inputs().capture_legacy_env())
}

#[ported(source = "src/process/unix.c", original = "moonbitlang_async_env_block_length")]
#[cfg(unix)]
pub(super) fn env_block_length(
    context: &mut ImportContext<'_>,
    env: u64,
) -> AsyncHostResult<u32> {
    context.host.process_inputs().legacy_env_length(env)
}

#[ported(source = "src/process/windows.c", original = "moonbitlang_async_env_block_length")]
#[cfg(windows)]
pub(super) fn env_block_length(
    context: &mut ImportContext<'_>,
    env: u64,
) -> AsyncHostResult<u32> {
    context.host.process_inputs().legacy_env_length(env)
}

#[ported(source = "src/process/unix.c", original = "moonbitlang_async_allocate_env_block")]
#[cfg(unix)]
pub(super) fn allocate_env_block(
    context: &mut ImportContext<'_>,
    size: u32,
) -> AsyncHostResult<u64> {
    context.host.process_inputs().allocate_legacy_env(size)
}

#[ported(source = "src/process/windows.c", original = "moonbitlang_async_allocate_env_block")]
#[cfg(windows)]
pub(super) fn allocate_env_block(
    context: &mut ImportContext<'_>,
    size: u32,
) -> AsyncHostResult<u64> {
    context.host.process_inputs().allocate_legacy_env(size)
}

#[compat(
    source = "src/process/wasm.mbt",
    original = "process/free_env",
    upstream_pr = 511,
    replacement = "spawn ownership transfer",
    no_op = true
)]
pub(super) fn free_env(_context: &mut ImportContext<'_>, _env: u64) -> AsyncHostResult<()> {
    Ok(())
}

#[compat(
    source = "src/process/wasm.mbt",
    original = "process/free_argv",
    upstream_pr = 511,
    replacement = "spawn ownership transfer",
    no_op = true
)]
#[cfg(unix)]
pub(super) fn free_argv(_context: &mut ImportContext<'_>, _argv: u64) -> AsyncHostResult<()> {
    Ok(())
}

#[ported(source = "src/process/unix.c", original = "moonbitlang_async_write_env_block")]
#[cfg(unix)]
pub(super) fn write_env_block(
    context: &mut ImportContext<'_>,
    dst: u64,
    src: u64,
) -> AsyncHostResult<()> {
    context.host.process_inputs().transfer_legacy_env(dst, src)
}

#[ported(source = "src/process/windows.c", original = "moonbitlang_async_write_env_block")]
#[cfg(windows)]
pub(super) fn write_env_block(
    context: &mut ImportContext<'_>,
    dst: u64,
    src: u64,
) -> AsyncHostResult<()> {
    context.host.process_inputs().transfer_legacy_env(dst, src)
}

#[ported(source = "src/process/unix.c", original = "moonbitlang_async_env_block_add_entry")]
#[cfg(unix)]
pub(super) fn env_block_add_entry(
    context: &mut ImportContext<'_>,
    env: u64,
    index: u32,
    key: u32,
    key_len: u32,
    value: u32,
    value_len: u32,
) -> AsyncHostResult<()> {
    let key = read_guest_os_string(context, key, key_len)?;
    let value = read_guest_os_string(context, value, value_len)?;
    context.host.process_inputs().add_legacy_env(env, index, key, value)
}

#[ported(source = "src/process/windows.c", original = "moonbitlang_async_env_block_add_entry")]
#[cfg(windows)]
pub(super) fn env_block_add_entry(
    context: &mut ImportContext<'_>,
    env: u64,
    offset: u32,
    key: u32,
    key_len: u32,
    value: u32,
    value_len: u32,
) -> AsyncHostResult<()> {
    let key = read_guest_u16(context, key, key_len)?;
    let value = read_guest_u16(context, value, value_len)?;
    context
        .host.process_inputs()
        .add_legacy_env(env, offset, &key, &value)
}

#[ported(source = "src/process/wasm.mbt", original = "process/make_env")]
#[cfg(unix)]
pub(super) fn make_env(
    context: &mut ImportContext<'_>,
    inherit_env: i32,
) -> AsyncHostResult<u64> {
    Ok(context.host.process_inputs().make_env(inherit_env != 0))
}

#[ported(source = "src/process/wasm.mbt", original = "process/make_env")]
#[cfg(windows)]
pub(super) fn make_env(
    context: &mut ImportContext<'_>,
    inherit_env: i32,
) -> AsyncHostResult<u64> {
    Ok(context.host.process_inputs().make_env(inherit_env != 0))
}

#[ported(
    source = "src/process/unix.c",
    original = "moonbitlang_async_env_block_add_entry"
)]
#[cfg(unix)]
pub(super) fn env_add_entry(
    context: &mut ImportContext<'_>,
    env: u64,
    key: u32,
    key_len: u32,
    value: u32,
    value_len: u32,
) -> AsyncHostResult<()> {
    let key = read_guest_os_string(context, key, key_len)?;
    let value = read_guest_os_string(context, value, value_len)?;
    context.host.process_inputs().add_env(env, key, value)
}

#[ported(
    source = "src/process/windows.c",
    original = "moonbitlang_async_env_block_add_entry"
)]
#[cfg(windows)]
pub(super) fn env_add_entry(
    context: &mut ImportContext<'_>,
    env: u64,
    key: u32,
    key_len: u32,
    value: u32,
    value_len: u32,
) -> AsyncHostResult<()> {
    let key = read_guest_os_string(context, key, key_len)?;
    let value = read_guest_os_string(context, value, value_len)?;
    context
        .host.process_inputs()
        .add_env(env, key, value)
}

#[ported(source = "src/process/unix.c", original = "moonbitlang_async_make_argv_array")]
#[cfg(unix)]
pub(super) fn make_argv_array_unix(
    context: &mut ImportContext<'_>,
    len: u32,
) -> AsyncHostResult<u64> {
    context.host.process_inputs().allocate_argv(len)
}

#[ported(
    source = "src/process/unix.c",
    original = "moonbitlang_async_argv_array_add_encoded_entry"
)]
#[cfg(unix)]
pub(super) fn argv_array_add_encoded_entry_unix(
    context: &mut ImportContext<'_>,
    argv: u64,
    index: u32,
    arg: u32,
    arg_len: u32,
) -> AsyncHostResult<()> {
    let arg = read_guest_os_string(context, arg, arg_len)?;
    context.host.process_inputs().add_argv(argv, index, arg)
}

#[ported(source = "src/process/unix.c", original = "moonbitlang_async_argv_array_add_entry")]
#[cfg(unix)]
pub(super) fn argv_array_add_entry_unix(
    context: &mut ImportContext<'_>,
    argv: u64,
    index: u32,
    arg: u32,
    arg_len: u32,
) -> AsyncHostResult<()> {
    let arg = read_guest_os_string(context, arg, arg_len)?;
    context.host.process_inputs().add_argv(argv, index, arg)
}

#[ported(
    source = "src/internal/event_loop/process.c",
    original = "moonbitlang_async_open_pid_handle"
)]
pub(super) fn open_pid_handle(
    context: &mut ImportContext<'_>,
    pid: i32,
) -> AsyncHostResult<u64> {
    let result = context.host.open_process_handle(pid);
    // Preserve the existing Linux ABI fallback without recording the pidfd
    // error. Other successful PID-only targets clear errno in AsyncHost.
    #[cfg(target_os = "linux")]
    if matches!(result, Err(AsyncHostError::Native(errno)) if errno == libc::ENOSYS || errno == libc::EPERM) {
        return Ok(context.host.invalid_fd());
    }
    result
}

#[ported(
    source = "src/internal/event_loop/process.c",
    original = "moonbitlang_async_get_process_result"
)]
pub(super) fn get_process_result(
    context: &mut ImportContext<'_>,
    handle: u64,
    pid: i32,
    out: u32,
) -> i32 {
    let result = context.host.read_process_exit(handle, pid, |code| {
        context.with_memory_mut(|memory| Ok(memory.write_exact(out, &code.to_le_bytes())?))
    });
    match result {
        Ok(()) => 0,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

#[ported(
    source = "src/process/unix.c",
    original = "moonbitlang_async_terminate_process"
)]
#[cfg(unix)]
pub(super) fn terminate(
    context: &mut ImportContext<'_>,
    pid: i32,
    signal: i32,
) -> AsyncHostResult<()> {
    context.host.process().terminate(pid, signal)
}

#[ported(
    source = "src/process/windows.c",
    original = "moonbitlang_async_terminate_process"
)]
#[cfg(windows)]
pub(super) fn terminate(
    context: &mut ImportContext<'_>,
    pid: i32,
    signal: i32,
) -> AsyncHostResult<()> {
    context.host.process().terminate(pid, signal)
}

#[ported(
    source = "src/process/unix.c",
    original = "moonbitlang_async_kill_process"
)]
#[cfg(unix)]
pub(super) fn kill(context: &mut ImportContext<'_>, pid: i32) -> AsyncHostResult<()> {
    context.host.process().kill(pid)
}

#[ported(
    source = "src/process/windows.c",
    original = "moonbitlang_async_kill_process"
)]
#[cfg(windows)]
pub(super) fn kill(context: &mut ImportContext<'_>, pid: i32) -> AsyncHostResult<()> {
    context.host.process().kill(pid)
}
}

#[cfg(windows)]
fn read_guest_u16(
    context: &mut ImportContext<'_>,
    ptr: u32,
    len: u32,
) -> AsyncHostResult<Vec<u16>> {
    context.with_memory_mut(|memory| read_u16(memory, ptr, len))
}
