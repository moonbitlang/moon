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

//! Only decode Wasm arguments here. WASI semantics and runtime state remain in
//! the existing backend-neutral host implementation.

use super::{
    StoreData,
    api::{self, Caller, HostFunc, Kind, Module, Store, Value},
};
use crate::wasi::{self, result_to_errno};

pub(super) fn imports<'s>(
    store: &'s Store<StoreData>,
    module: &Module<'s, StoreData>,
) -> api::Result<Vec<HostFunc<'s, StoreData>>> {
    type Host = fn(Caller<'_, StoreData>, &[Value]) -> api::Result<Vec<Value>>;
    module
        .imports()?
        .into_iter()
        .map(|import| {
            // Signatures are checked before any callback can index or decode its
            // arguments. Imports are provided in the module's declaration order.
            let (params, results, callback): (&[Kind], &[Kind], Host) =
                match (import.module.as_str(), import.name.as_str()) {
                    ("wasi_snapshot_preview1", "fd_write") => {
                        (&[Kind::I32; 4], &[Kind::I32], fd_write)
                    }
                    ("wasi_snapshot_preview1", "proc_exit") => (&[Kind::I32], &[], proc_exit),
                    ("wasi_snapshot_preview1", "args_sizes_get") => {
                        (&[Kind::I32; 2], &[Kind::I32], args_sizes_get)
                    }
                    ("wasi_snapshot_preview1", "args_get") => {
                        (&[Kind::I32; 2], &[Kind::I32], args_get)
                    }
                    _ => {
                        return Err(api::Error::Interface(format!(
                            "unsupported wee8 import {}::{}",
                            import.module, import.name
                        )));
                    }
                };
            if import.params != params || import.results != results {
                return Err(api::Error::Interface(format!(
                    "incorrect signature for {}::{}",
                    import.module, import.name
                )));
            }
            store.host_func(params, results, callback)
        })
        .collect()
}

fn fd_write(caller: Caller<'_, StoreData>, args: &[Value]) -> api::Result<Vec<Value>> {
    let [
        Value::I32(fd),
        Value::I32(iovs),
        Value::I32(count),
        Value::I32(written),
    ] = args
    else {
        unreachable!("checked import signature")
    };
    let errno = caller.with_memory(|data, memory| {
        result_to_errno(wasi::fd_write_impl(
            &data.wasi,
            memory,
            *fd,
            *iovs as u32,
            *count as u32,
            *written as u32,
        ))
    })?;
    Ok(vec![Value::I32(errno)])
}

fn proc_exit(caller: Caller<'_, StoreData>, args: &[Value]) -> api::Result<Vec<Value>> {
    let [Value::I32(code)] = args else {
        unreachable!("checked import signature")
    };
    caller.with_data(|data| wasi::proc_exit_impl(&data.wasi, *code as u32));
    // A trap unwinds the guest; the runner consumes the separately recorded
    // termination request, so proc_exit(0) is not mistaken for a guest failure.
    Err(api::Error::Host("WASI proc_exit".into()))
}

fn args_sizes_get(caller: Caller<'_, StoreData>, args: &[Value]) -> api::Result<Vec<Value>> {
    let [Value::I32(argc), Value::I32(size)] = args else {
        unreachable!("checked import signature")
    };
    let errno = caller.with_memory(|data, memory| {
        result_to_errno(wasi::args_sizes_get_impl(
            &data.wasi,
            memory,
            *argc as u32,
            *size as u32,
        ))
    })?;
    Ok(vec![Value::I32(errno)])
}

fn args_get(caller: Caller<'_, StoreData>, args: &[Value]) -> api::Result<Vec<Value>> {
    let [Value::I32(argv), Value::I32(bytes)] = args else {
        unreachable!("checked import signature")
    };
    let errno = caller.with_memory(|data, memory| {
        result_to_errno(wasi::args_get_impl(
            &data.wasi,
            memory,
            *argv as u32,
            *bytes as u32,
        ))
    })?;
    Ok(vec![Value::I32(errno)])
}
