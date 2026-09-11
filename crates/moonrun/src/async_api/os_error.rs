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

use crate::async_host::AsyncHostResult;
use crate::async_sys::os_error::stub;

use super::context::ImportContext;
use super::provenance::ported_imports;

ported_imports! {
#[ported(source = "src/os_error/stub.c")]
pub(super) fn get_errno(context: &mut ImportContext<'_, '_>) -> i32 {
    stub::get_errno(context.host)
}

#[compat(
    source = "src/os_error/stub.c",
    original = "moonbitlang_async_is_nonblocking_io_error",
    upstream_pr = 566,
    replacement = "platform errno constants in src/os_error/error.mbt"
)]
pub(super) fn is_nonblocking_io_error(_context: &mut ImportContext<'_, '_>, errno: i32) -> i32 {
    if stub::is_nonblocking_io_error(errno) {
        1
    } else {
        0
    }
}

#[compat(
    source = "src/os_error/stub.c",
    original = "moonbitlang_async_is_EINTR",
    upstream_pr = 566,
    replacement = "platform errno constants in src/os_error/error.mbt"
)]
pub(super) fn is_eintr(_context: &mut ImportContext<'_, '_>, errno: i32) -> i32 {
    if stub::is_eintr(errno) { 1 } else { 0 }
}

#[compat(
    source = "src/os_error/stub.c",
    original = "moonbitlang_async_is_ENOENT",
    upstream_pr = 566,
    replacement = "platform errno constants in src/os_error/error.mbt"
)]
pub(super) fn is_enoent(_context: &mut ImportContext<'_, '_>, errno: i32) -> i32 {
    if stub::is_enoent(errno) { 1 } else { 0 }
}

#[compat(
    source = "src/os_error/stub.c",
    original = "moonbitlang_async_is_EEXIST",
    upstream_pr = 566,
    replacement = "platform errno constants in src/os_error/error.mbt"
)]
pub(super) fn is_eexist(_context: &mut ImportContext<'_, '_>, errno: i32) -> i32 {
    if stub::is_eexist(errno) { 1 } else { 0 }
}

#[compat(
    source = "src/os_error/stub.c",
    original = "moonbitlang_async_is_EACCES",
    upstream_pr = 566,
    replacement = "platform errno constants in src/os_error/error.mbt"
)]
pub(super) fn is_eacces(_context: &mut ImportContext<'_, '_>, errno: i32) -> i32 {
    if stub::is_eacces(errno) { 1 } else { 0 }
}

#[compat(
    source = "src/os_error/stub.c",
    original = "moonbitlang_async_is_ECONNREFUSED",
    upstream_pr = 566,
    replacement = "platform errno constants in src/os_error/error.mbt"
)]
pub(super) fn is_econnrefused(_context: &mut ImportContext<'_, '_>, errno: i32) -> i32 {
    if stub::is_econnrefused(errno) { 1 } else { 0 }
}

#[compat(
    source = "src/os_error/stub.c",
    original = "moonbitlang_async_is_ERROR_NOTIFY_ENUM_DIR",
    upstream_pr = 566,
    replacement = "platform errno constants in src/os_error/error.mbt"
)]
pub(super) fn is_error_notify_enum_dir(_context: &mut ImportContext<'_, '_>, errno: i32) -> i32 {
    if stub::is_error_notify_enum_dir(errno) {
        1
    } else {
        0
    }
}

#[ported(source = "src/os_error/stub.c")]
pub(super) fn errno_to_string(context: &mut ImportContext<'_, '_>, errno: i32) -> u64 {
    context.host.insert_c_buffer(stub::errno_to_string(errno))
}

pub(super) fn free_errno_str(context: &mut ImportContext<'_, '_>, ptr: u64) -> AsyncHostResult<()> {
    super::c_buffer::free(context, ptr)
}

#[compat(
    source = "src/os_error/stub.c",
    original = "moonbitlang_async_get_ENOTDIR",
    upstream_pr = 566,
    replacement = "platform errno constants in src/os_error/error.mbt"
)]
pub(super) fn get_enotdir(_context: &mut ImportContext<'_, '_>) -> i32 {
    stub::get_enotdir()
}

#[compat(
    source = "src/os_error/stub.c",
    original = "moonbitlang_async_get_ENOTSUP",
    upstream_pr = 566,
    replacement = "platform errno constants in src/os_error/error.mbt"
)]
pub(super) fn get_enotsup(_context: &mut ImportContext<'_, '_>) -> i32 {
    stub::get_enotsup()
}

}
