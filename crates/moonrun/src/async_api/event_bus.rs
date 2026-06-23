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

use super::context::ImportContext;
#[cfg(test)]
use super::provenance::{PortedImport, SourceLocation, SourceRoot};

#[cfg(all(test, target_os = "linux"))]
const EVENT_BUS_SOURCE: &str = "src/internal/event_loop/epoll.c";
#[cfg(all(test, target_os = "macos"))]
const EVENT_BUS_SOURCE: &str = "src/internal/event_loop/kqueue.c";
#[cfg(all(test, windows))]
const EVENT_BUS_SOURCE: &str = "src/internal/event_loop/iocp.c";
#[cfg(all(test, windows))]
const IOCP_SOURCE: &str = "src/internal/event_loop/iocp.c";
#[cfg(test)]
const EVENT_BUS_SOURCES: &[SourceLocation] = &[SourceLocation {
    root: SourceRoot::MoonbitAsync,
    path: EVENT_BUS_SOURCE,
}];
#[cfg(all(test, windows))]
const IOCP_SOURCES: &[SourceLocation] = &[SourceLocation {
    root: SourceRoot::MoonbitAsync,
    path: IOCP_SOURCE,
}];

#[cfg(test)]
pub(super) const PORTED_IMPORTS: &[PortedImport] = &[
    ported_from(
        EVENT_BUS_SOURCES,
        "create",
        "moonbitlang_async_event_bus_create",
    ),
    ported_from(
        EVENT_BUS_SOURCES,
        "destroy",
        "moonbitlang_async_event_bus_destroy",
    ),
    ported_from(
        EVENT_BUS_SOURCES,
        "register",
        "moonbitlang_async_event_bus_register",
    ),
    ported_from(
        EVENT_BUS_SOURCES,
        "wait",
        "moonbitlang_async_event_bus_wait",
    ),
    ported_from(
        EVENT_BUS_SOURCES,
        "get_event",
        "moonbitlang_async_event_list_get",
    ),
    ported_from(
        EVENT_BUS_SOURCES,
        "event_fd",
        "moonbitlang_async_event_get_fd",
    ),
    #[cfg(unix)]
    ported_from(
        EVENT_BUS_SOURCES,
        "event_events",
        "moonbitlang_async_event_get_events",
    ),
    #[cfg(windows)]
    ported_from(
        IOCP_SOURCES,
        "event_io_result",
        "moonbitlang_async_event_get_io_result",
    ),
    #[cfg(windows)]
    ported_from(
        IOCP_SOURCES,
        "event_bytes_transferred",
        "moonbitlang_async_event_get_bytes_transferred",
    ),
];

#[cfg(test)]
const fn ported_from(
    sources: &'static [SourceLocation],
    rust_symbol: &'static str,
    native_symbol: &'static str,
) -> PortedImport {
    PortedImport {
        rust_module: module_path!(),
        rust_symbol,
        native_symbol: Some(native_symbol),
        sources,
    }
}

pub(super) fn create(context: &mut ImportContext) -> AsyncHostResult<u64> {
    context.host.poll_create()
}

pub(super) fn destroy(context: &mut ImportContext, bus: u64) -> AsyncHostResult<()> {
    context.host.poll_destroy(bus)
}

pub(super) fn register(context: &mut ImportContext, bus: u64, fd: u64, read_only: i32) -> i32 {
    poll_errno_result(context, context.host.poll_register(bus, fd, read_only != 0))
}

pub(super) fn wait(context: &mut ImportContext, bus: u64, timeout_ms: i32) -> i32 {
    match context.host.poll_wait(bus, timeout_ms) {
        Ok(n) => n,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

pub(super) fn get_event(context: &mut ImportContext, bus: u64, index: i32) -> AsyncHostResult<u64> {
    context.host.poll_get_event(bus, index)
}

pub(super) fn event_fd(context: &mut ImportContext, event: u64) -> AsyncHostResult<u64> {
    context.host.poll_event_fd(event)
}

#[cfg(unix)]
pub(super) fn event_events(context: &mut ImportContext, event: u64) -> AsyncHostResult<i32> {
    context.host.poll_event_events(event)
}

#[cfg(windows)]
pub(super) fn event_io_result(context: &mut ImportContext, event: u64) -> AsyncHostResult<u64> {
    context.host.poll_event_io_result(event)
}

#[cfg(windows)]
pub(super) fn event_bytes_transferred(
    context: &mut ImportContext,
    event: u64,
) -> AsyncHostResult<i32> {
    context.host.poll_event_bytes_transferred(event)
}

fn poll_errno_result(context: &ImportContext, result: AsyncHostResult<()>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}
