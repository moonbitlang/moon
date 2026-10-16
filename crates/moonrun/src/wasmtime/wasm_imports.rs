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

//! Wasmtime signature, caller borrowing and trap conversion for shared imports.

use super::StoreData;
use crate::wasm_imports::{Caller, HostFunction, ImportRegistry, WasmParams, WasmResult, WasmType};

pub(super) struct Registry<'a, T> {
    pub(super) linker: &'a mut wasmtime::Linker<StoreData>,
    pub(super) data: fn(&StoreData) -> &T,
}

impl<T: 'static> ImportRegistry<T> for Registry<'_, T> {
    type Error = wasmtime::Error;
    fn func_wrap<P: WasmParams, R: WasmResult, F: HostFunction<T, P, R>>(
        &mut self,
        module: &'static str,
        name: &'static str,
        function: F,
    ) -> wasmtime::Result<()> {
        let data = self.data;
        let val_type = |kind| match kind {
            WasmType::I32 => wasmtime::ValType::I32,
            WasmType::I64 => wasmtime::ValType::I64,
            WasmType::F32 => wasmtime::ValType::F32,
            WasmType::F64 => wasmtime::ValType::F64,
        };
        let ty = wasmtime::FuncType::new(
            self.linker.engine(),
            P::TYPES.iter().copied().map(val_type),
            R::TYPE.map(val_type),
        );
        self.linker
            .func_new(module, name, ty, move |mut caller, args, results| {
                let params = P::decode::<std::convert::Infallible>(|index, kind| {
                    Ok(match kind {
                        WasmType::I32 => args[index].i32().expect("declared i32 import") as u64,
                        WasmType::I64 => args[index].i64().expect("declared i64 import") as u64,
                        WasmType::F32 => {
                            u64::from(args[index].f32().expect("declared f32 import").to_bits())
                        }
                        WasmType::F64 => args[index].f64().expect("declared f64 import").to_bits(),
                    })
                })
                .unwrap();
                let memory = caller
                    .get_export("memory")
                    .and_then(wasmtime::Extern::into_memory);
                let value = match memory {
                    Some(memory) => {
                        let (bytes, state) = memory.data_and_store_mut(&mut caller);
                        function.call(
                            Caller {
                                data: data(state),
                                termination: state.termination_request(),
                                memory: Ok(bytes),
                            },
                            params,
                        )
                    }
                    // Preserve Wasmtime's existing empty-memory behavior.
                    None => function.call(
                        Caller {
                            data: data(caller.data()),
                            termination: caller.data().termination_request(),
                            memory: Ok(&mut []),
                        },
                        params,
                    ),
                }
                .map_err(|error| error.at(module, name))?;
                if let (Some(kind), Some(bits)) = (R::TYPE, value.into_bits()) {
                    results[0] = match kind {
                        WasmType::I32 => wasmtime::Val::I32(bits as i32),
                        WasmType::I64 => wasmtime::Val::I64(bits as i64),
                        WasmType::F32 => wasmtime::Val::F32(bits as u32),
                        WasmType::F64 => wasmtime::Val::F64(bits),
                    };
                }
                Ok(())
            })?;
        Ok(())
    }
}
