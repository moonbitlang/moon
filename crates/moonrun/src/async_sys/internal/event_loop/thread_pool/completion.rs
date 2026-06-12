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

use std::collections::VecDeque;

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::ported_fns;

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_fetch_completion"
    )]
    #[allow(dead_code)]
    pub(crate) fn fetch_completion(
        completions: &mut VecDeque<i32>,
        output: &mut [i32],
    ) -> AsyncHostResult<i32> {
        let n = output.len().min(completions.len());
        for slot in &mut output[..n] {
            *slot = completions.pop_front().ok_or(AsyncHostError::Inval)?;
        }
        i32::try_from(n * std::mem::size_of::<i32>()).map_err(|_| AsyncHostError::Fault)
    }
}
