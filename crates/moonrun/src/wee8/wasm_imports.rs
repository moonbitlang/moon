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

//! Resolve shared definitions in the module's import order for the Wasm C API.

use super::{StoreData, api};
use crate::wasm_imports::{
    Caller, HostFunction, ImportError, ImportRegistry, WasmParams, WasmResult, WasmType,
};
use std::rc::Rc;

pub(super) struct Registry<'s, T> {
    store: &'s api::Store<StoreData>,
    imports: Vec<api::Import>,
    functions: Vec<Option<api::HostFunc<'s, StoreData>>>,
    data: fn(&StoreData) -> &T,
}

impl<'s, T> Registry<'s, T> {
    pub(super) fn new(
        store: &'s api::Store<StoreData>,
        module: &api::Module<'s, StoreData>,
        data: fn(&StoreData) -> &T,
    ) -> api::Result<Self> {
        let imports = module.imports()?;
        let functions = (0..imports.len()).map(|_| None).collect();
        Ok(Self {
            store,
            imports,
            functions,
            data,
        })
    }

    pub(super) fn finish(self) -> api::Result<Vec<api::HostFunc<'s, StoreData>>> {
        self.imports
            .into_iter()
            .zip(self.functions)
            .map(|(import, function)| {
                function.ok_or_else(|| {
                    api::Error::Interface(format!(
                        "unsupported wee8 import {}::{}",
                        import.module, import.name
                    ))
                })
            })
            .collect()
    }
}

impl<T: 'static> ImportRegistry<T> for Registry<'_, T> {
    type Error = api::Error;
    fn func_wrap<P: WasmParams, R: WasmResult, F: HostFunction<T, P, R>>(
        &mut self,
        module: &'static str,
        name: &'static str,
        function: F,
    ) -> api::Result<()> {
        let data = self.data;
        // A module can import the same definition more than once. All slots
        // retain the same callback and its captures until their store is dropped.
        let function = Rc::new(function);
        let params: Vec<_> = P::TYPES
            .iter()
            .map(|kind| match kind {
                WasmType::I32 => api::Kind::I32,
                WasmType::I64 => api::Kind::I64,
            })
            .collect();
        let results: Vec<_> = R::HAS_RESULT
            .then_some(api::Kind::I32)
            .into_iter()
            .collect();
        for (import, slot) in self.imports.iter().zip(&mut self.functions) {
            if import.module != module || import.name != name {
                continue;
            }
            if import.params != params || import.results != results {
                return Err(api::Error::Interface(format!(
                    "incorrect signature for {module}::{name}"
                )));
            }
            let function = Rc::clone(&function);
            *slot = Some(
                self.store
                    .host_func(&params, &results, move |caller, args| {
                        let params = P::decode::<std::convert::Infallible>(|index, kind| {
                            Ok(match (kind, args[index]) {
                                (WasmType::I32, api::Value::I32(value)) => i64::from(value),
                                (WasmType::I64, api::Value::I64(value)) => value,
                                _ => unreachable!("checked integer import signature"),
                            })
                        })
                        .unwrap();
                        // with_memory fails before invoking the operation. Invoke it
                        // exactly once with deferred failure so memory-free calls such
                        // as proc_exit still work during module instantiation.
                        let result = match caller.with_memory(|state, memory| {
                            function.call(
                                Caller {
                                    data: data(state),
                                    memory: Ok(memory),
                                },
                                params,
                            )
                        }) {
                            Ok(result) => result,
                            Err(error) => caller.with_data(|state| {
                                function.call(
                                    Caller {
                                        data: data(state),
                                        memory: Err(ImportError::Host(error.into())),
                                    },
                                    params,
                                )
                            }),
                        };
                        result
                            .map(|value| {
                                value.into_i32().map(api::Value::I32).into_iter().collect()
                            })
                            .map_err(|error| api::Error::Host(error.to_string()))
                    })?,
            );
        }
        Ok(())
    }
}
