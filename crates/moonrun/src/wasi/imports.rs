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

//! The shared WASIp1 subset currently supported by all three engine adapters.

use super::{WASI_ERRNO_FAULT, WasiContext, WasiResult, result_to_errno};
use crate::wasm_imports::{Caller, ImportError, ImportRegistry};

pub(crate) const MODULE: &str = "wasi_snapshot_preview1";

pub(crate) fn register<R: ImportRegistry<WasiContext>>(registry: &mut R) -> Result<(), R::Error> {
    registry.define(MODULE, "args_sizes_get", |caller, [argc, size]| {
        with_memory(caller, |data, memory| {
            super::args_sizes_get_impl(data, memory, argc as u32, size as u32)
        })
    })?;
    registry.define(MODULE, "args_get", |caller, [argv, bytes]| {
        with_memory(caller, |data, memory| {
            super::args_get_impl(data, memory, argv as u32, bytes as u32)
        })
    })?;
    registry.define(MODULE, "fd_write", |caller, [fd, iovs, count, written]| {
        with_memory(caller, |data, memory| {
            super::fd_write_impl(data, memory, fd, iovs as u32, count as u32, written as u32)
        })
    })?;
    registry.define(
        MODULE,
        "proc_exit",
        |caller, [code]| -> Result<(), ImportError> {
            super::proc_exit_impl(caller.data, code as u32);
            Err(ImportError::Terminated)
        },
    )
}

fn with_memory(
    caller: Caller<'_, WasiContext>,
    operation: impl FnOnce(&WasiContext, &mut [u8]) -> WasiResult<()>,
) -> Result<i32, ImportError> {
    // V8 has historically returned EFAULT for an unbound memory. wee8 instead
    // reports a host error for memory access during module start. Preserve that
    // adapter limitation without imposing it on memory-free operations.
    match caller.memory {
        Ok(memory) => Ok(result_to_errno(operation(caller.data, memory))),
        Err(ImportError::MemoryUnavailable) => Ok(WASI_ERRNO_FAULT),
        Err(error) => Err(error),
    }
}
