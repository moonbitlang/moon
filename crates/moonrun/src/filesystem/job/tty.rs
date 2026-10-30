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

//! Terminal read Jobs, following `src/internal/event_loop/tty.c` on async's tty branch.
//! Only Windows needs one: on Unix the guest reads the terminal through the
//! event bus.

#[cfg(windows)]
mod windows;

#[cfg(unix)]
use crate::async_host::{AsyncHostError, AsyncHostResult};
#[cfg(unix)]
use crate::resource::Resource;

/// Read console input records as UTF-16. Return `TTY_READ_RESIZED` for a
/// resize without input; a resize read along with input sets `resized`.
///
/// Like native, Unix reports this operation as unavailable.
#[cfg(unix)]
pub(super) fn read_console_input(
    _input: &Resource,
    _len: u32,
    _result: &mut Option<Vec<u16>>,
    _resized: &mut bool,
) -> AsyncHostResult<i64> {
    Err(AsyncHostError::Native(libc::ENOSYS))
}

#[cfg(windows)]
pub(super) use windows::{make_cancel_event, read_console_input};
