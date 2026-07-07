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

use crate::async_host::AsyncHostResult;
use crate::async_sys::process;

use super::context::ImportContext;
use super::provenance::ported_imports;

ported_imports! {
#[cfg(unix)]
pub(super) fn open_pid_handle(context: &mut ImportContext<'_, '_>, _pid: i32) -> u64 {
    context.host.invalid_fd()
}

#[ported(
    source = "src/internal/event_loop/process.c",
    original = "moonbitlang_async_open_pid_handle"
)]
#[cfg(windows)]
pub(super) fn open_pid_handle(
    context: &mut ImportContext<'_, '_>,
    pid: i32,
) -> AsyncHostResult<u64> {
    process::open_pid_handle(pid).map(|handle| context.host.insert_host_file_handle(handle))
}

#[ported(
    source = "src/process/unix.c",
    original = "moonbitlang_async_terminate_process"
)]
#[cfg(unix)]
pub(super) fn terminate(
    _context: &mut ImportContext<'_, '_>,
    pid: i32,
    signal: i32,
) -> AsyncHostResult<()> {
    process::terminate_process(pid, signal)
}

#[ported(
    source = "src/process/windows.c",
    original = "moonbitlang_async_terminate_process"
)]
#[cfg(windows)]
pub(super) fn terminate(
    _context: &mut ImportContext<'_, '_>,
    pid: i32,
    signal: i32,
) -> AsyncHostResult<()> {
    process::terminate_process(pid, signal)
}

#[ported(
    source = "src/process/unix.c",
    original = "moonbitlang_async_kill_process"
)]
#[cfg(unix)]
pub(super) fn kill(_context: &mut ImportContext<'_, '_>, pid: i32) -> AsyncHostResult<()> {
    process::kill_process(pid)
}

#[ported(
    source = "src/process/windows.c",
    original = "moonbitlang_async_kill_process"
)]
#[cfg(windows)]
pub(super) fn kill(_context: &mut ImportContext<'_, '_>, pid: i32) -> AsyncHostResult<()> {
    process::kill_process(pid)
}
}
