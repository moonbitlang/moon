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

use rand::{RngCore, rngs::OsRng};

use crate::async_host::AsyncHostError;
use crate::guest_memory::GuestMemory;

use super::context::ImportContext;

pub(super) fn fill(context: &mut ImportContext<'_, '_>, buffer: u32, length: u32) -> i32 {
    let result = context.with_memory_mut(|memory| {
        let destination = memory.read_exact_mut(buffer, length)?;
        OsRng.try_fill_bytes(destination).map_err(|error| {
            error
                .raw_os_error()
                .map_or(AsyncHostError::Io, AsyncHostError::Native)
        })
    });
    match result {
        Ok(()) => 0,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}
