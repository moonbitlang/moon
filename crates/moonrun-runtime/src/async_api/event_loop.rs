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

#[cfg(windows)]
use crate::async_sys::internal::event_loop::io;
use crate::async_sys::internal::event_loop::thread_pool;

use super::context::ImportContext;
use super::provenance::ported_imports;

ported_imports! {
#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn get_platform(_context: &mut ImportContext<'_, '_>) -> i32 {
    thread_pool::get_platform()
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn errno_is_cancelled(_context: &mut ImportContext<'_, '_>, errno: i32) -> i32 {
    if thread_pool::errno_is_cancelled(errno) {
        1
    } else {
        0
    }
}

#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_init_WSA"
)]
#[cfg(windows)]
pub(super) fn init_wsa(_context: &mut ImportContext<'_, '_>) -> i32 {
    io::init_wsa()
}

#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_cleanup_WSA"
)]
#[cfg(windows)]
pub(super) fn cleanup_wsa(_context: &mut ImportContext<'_, '_>) -> i32 {
    io::cleanup_wsa()
}
}
