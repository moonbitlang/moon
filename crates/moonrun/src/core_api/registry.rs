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

//! All `moonbitlang/core` import names and signatures, shared by every engine.

use super::{
    MOONBIT_CORE_MODULE, buffer_to_handle, current_exe, current_exe_dir, environment, windows_time,
};
use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::runtime::Runtime;
use crate::wasm_imports::{Caller, ImportError, ImportRegistry};

pub(crate) fn register<R: ImportRegistry<Runtime>>(registry: &mut R) -> Result<(), R::Error> {
    registry.func_wrap(
        MOONBIT_CORE_MODULE,
        "env/current_exe",
        |caller: Caller<'_, '_, Runtime>| -> Result<u64, ImportError> {
            Ok(caller.with_data(current_exe))
        },
    )?;
    registry.func_wrap(
        MOONBIT_CORE_MODULE,
        "env/current_exe_dir",
        |caller: Caller<'_, '_, Runtime>| -> Result<u64, ImportError> {
            Ok(caller.with_data(current_exe_dir))
        },
    )?;
    registry.func_wrap(
        MOONBIT_CORE_MODULE,
        "env/current_dir",
        |caller: Caller<'_, '_, Runtime>| -> Result<u64, ImportError> {
            Ok(caller
                .with_data(|runtime| buffer_to_handle(runtime, environment::current_dir(runtime))))
        },
    )?;
    registry.func_wrap(
        MOONBIT_CORE_MODULE,
        "env/get_env_var",
        |caller: Caller<'_, '_, Runtime>, key_ptr: u32, key_len: u32| -> Result<u64, ImportError> {
            caller.with_memory(|caller| {
                let buffer = match caller.memory {
                    Ok(memory) => environment::get_env_var(caller.data, memory, key_ptr, key_len),
                    Err(ImportError::Terminated) => return Err(ImportError::Terminated),
                    Err(_) => Err(AsyncHostError::Fault),
                };
                Ok(buffer_to_handle(caller.data, buffer))
            })
        },
    )?;
    registry.func_wrap(
        MOONBIT_CORE_MODULE,
        "env/set_env_var",
        |caller: Caller<'_, '_, Runtime>,
         key_ptr: u32,
         key_len: u32,
         value_ptr: u32,
         value_len: u32|
         -> Result<i32, ImportError> {
            with_memory(caller, |runtime, memory| {
                environment::set_env_var(runtime, memory, key_ptr, key_len, value_ptr, value_len)
            })
        },
    )?;
    registry.func_wrap(
        MOONBIT_CORE_MODULE,
        "env/unset_env_var",
        |caller: Caller<'_, '_, Runtime>, key_ptr: u32, key_len: u32| -> Result<i32, ImportError> {
            with_memory(caller, |runtime, memory| {
                environment::unset_env_var(runtime, memory, key_ptr, key_len)
            })
        },
    )?;
    registry.func_wrap(
        MOONBIT_CORE_MODULE,
        "time/windows/get_dynamic_time_zone_information",
        |caller: Caller<'_, '_, Runtime>, out: u32| {
            with_memory(caller, |rt, mem| windows_time::capture(rt, mem, out))
        },
    )?;
    registry.func_wrap(
        MOONBIT_CORE_MODULE,
        "time/windows/get_time_zone_metadata",
        |caller: Caller<'_, '_, Runtime>, handle: u64, out: u32, capacity: u32| {
            with_memory(caller, |rt, mem| {
                windows_time::metadata(rt, handle, mem, out, capacity)
            })
        },
    )?;
    registry.func_wrap(
        MOONBIT_CORE_MODULE,
        "time/windows/get_current_time_zone_rule",
        |caller: Caller<'_, '_, Runtime>, handle: u64, out: u32| {
            with_memory(caller, |rt, mem| {
                windows_time::current_rule(rt, handle, mem, out)
            })
        },
    )?;
    registry.func_wrap(
        MOONBIT_CORE_MODULE,
        "time/windows/get_dynamic_time_zone_information_effective_years",
        |caller: Caller<'_, '_, Runtime>, handle: u64, out: u32| {
            with_memory(caller, |rt, mem| {
                windows_time::effective_years(rt, handle, mem, out)
            })
        },
    )?;
    registry.func_wrap(
        MOONBIT_CORE_MODULE,
        "time/windows/get_time_zone_information_for_year",
        |caller: Caller<'_, '_, Runtime>, handle: u64, year: u32, out: u32| {
            with_memory(caller, |rt, mem| {
                windows_time::for_year(rt, handle, year, mem, out)
            })
        },
    )?;
    registry.func_wrap(
        MOONBIT_CORE_MODULE,
        "time/windows/free_time_zone",
        |caller: Caller<'_, '_, Runtime>, handle: u64| -> Result<i32, ImportError> {
            Ok(caller.with_data(|rt| {
                windows_time::release(rt, handle).map_or_else(|e| e.errno(), |()| 0)
            }))
        },
    )
}

fn with_memory(
    caller: Caller<'_, '_, Runtime>,
    operation: impl FnOnce(&Runtime, &mut [u8]) -> AsyncHostResult<()>,
) -> Result<i32, ImportError> {
    caller.with_memory(|caller| match caller.memory {
        Ok(memory) => Ok(match operation(caller.data, memory) {
            Ok(()) => 0,
            Err(error) => error.errno(),
        }),
        Err(ImportError::Terminated) => Err(ImportError::Terminated),
        // Adapters report an unavailable/unborrowable memory differently.
        // These imports treat any failed memory acquisition as a bad address.
        Err(_) => Ok(AsyncHostError::Fault.errno()),
    })
}
