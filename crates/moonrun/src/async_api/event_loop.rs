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
}
