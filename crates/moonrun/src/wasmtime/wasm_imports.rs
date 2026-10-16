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
        let ty = wasmtime::FuncType::new(
            self.linker.engine(),
            P::TYPES.iter().map(|kind| match kind {
                WasmType::I32 => wasmtime::ValType::I32,
                WasmType::I64 => wasmtime::ValType::I64,
            }),
            R::HAS_RESULT.then_some(wasmtime::ValType::I32),
        );
        self.linker
            .func_new(module, name, ty, move |mut caller, args, results| {
                let params = P::decode::<std::convert::Infallible>(|index, kind| {
                    Ok(match kind {
                        WasmType::I32 => i64::from(args[index].i32().expect("declared i32 import")),
                        WasmType::I64 => args[index].i64().expect("declared i64 import"),
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
                                memory: Ok(bytes),
                            },
                            params,
                        )
                    }
                    // Preserve Wasmtime's existing empty-memory behavior.
                    None => function.call(
                        Caller {
                            data: data(caller.data()),
                            memory: Ok(&mut []),
                        },
                        params,
                    ),
                }?;
                if let Some(value) = value.into_i32() {
                    results[0] = wasmtime::Val::I32(value);
                }
                Ok(())
            })?;
        Ok(())
    }
}
