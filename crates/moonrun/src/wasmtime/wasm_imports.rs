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

//! Wasmtime signature conversion and scoped store access for shared imports.
use super::StoreData;
use crate::wasm_imports::{
    CallContext, Caller, ExternRef, HostFunction, ImportError, ImportRegistry, MemoryCaller, Value,
    WasmParams, WasmResult, WasmType, payload::PayloadContext,
};
use std::{any::Any, marker::PhantomData};

pub(super) struct Registry<'a, T> {
    pub(super) linker: &'a mut wasmtime::Linker<StoreData>,
    pub(super) data: fn(&StoreData) -> &T,
}
struct Context<'a, T> {
    caller: wasmtime::Caller<'a, StoreData>,
    data: fn(&StoreData) -> &T,
}
impl<'call, T> PayloadContext<'call> for Context<'_, T> {
    fn new_payload(
        &mut self,
        data: Box<dyn Any + Send + Sync>,
    ) -> Result<Option<ExternRef<'call>>, ImportError> {
        wasmtime::ExternRef::new(&mut self.caller, data)
            .map(|r| Some(ExternRef::Wasmtime(r, PhantomData)))
            .map_err(|e| ImportError::Host(e.into()))
    }
    fn with_payload(
        &self,
        value: &Option<ExternRef<'call>>,
        operation: &mut dyn FnMut(Option<&dyn Any>),
    ) -> Result<(), ImportError> {
        let Some(ExternRef::Wasmtime(value, _)) = value else {
            return Err(ImportError::Operation("invalid host reference".into()));
        };
        let data = value
            .data(&self.caller)
            .map_err(|e| ImportError::Host(e.into()))?;
        // Existing JS-string and ffi-bytes adapters create concrete payloads.
        // The shared, object-safe conversion interface supplies an owned box.
        operation(data.map(|data| -> &dyn Any {
            data.downcast_ref::<Box<dyn Any + Send + Sync>>()
                .map_or(data, |data| data.as_ref())
        }));
        Ok(())
    }
}
impl<'call, T> CallContext<'call, T> for Context<'_, T> {
    fn with_data(&self, operation: &mut dyn FnMut(&T)) {
        operation((self.data)(self.caller.data()));
    }
    fn with_memory(&mut self, operation: &mut dyn FnMut(MemoryCaller<'_, T>)) {
        let memory = self
            .caller
            .get_export("memory")
            .and_then(wasmtime::Extern::into_memory);
        match memory {
            Some(memory) => {
                let (bytes, state) = memory.data_and_store_mut(&mut self.caller);
                operation(MemoryCaller {
                    data: (self.data)(state),
                    memory: Ok(bytes),
                    termination: state.termination_request(),
                });
            }
            None => operation(MemoryCaller {
                data: (self.data)(self.caller.data()),
                memory: Ok(&mut []),
                termination: self.caller.data().termination_request(),
            }),
        }
    }
    fn terminate(&self, outcome: crate::run_termination::RunTermination) {
        self.caller.data().termination_request().request(outcome);
    }
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
        let ty = |kind| match kind {
            WasmType::I32 => wasmtime::ValType::I32,
            WasmType::I64 => wasmtime::ValType::I64,
            WasmType::F32 => wasmtime::ValType::F32,
            WasmType::F64 => wasmtime::ValType::F64,
            WasmType::ExternRef => wasmtime::ValType::EXTERNREF,
        };
        let signature = wasmtime::FuncType::new(
            self.linker.engine(),
            P::TYPES.iter().copied().map(ty),
            R::TYPE.map(ty),
        );
        self.linker
            .func_new(module, name, signature, move |caller, args, results| {
                // Wasmtime's host-call trampoline enters a GC root scope before
                // decoding arguments and exits it after encoding results. Retain its
                // roots through that transfer; an inner RootScope would end too soon.
                let mut context = Context { caller, data };
                let params = P::decode::<std::convert::Infallible>(|index, kind| {
                    Ok(match kind {
                        WasmType::I32 => Value::I32(args[index].i32().unwrap() as u32),
                        WasmType::I64 => Value::I64(args[index].i64().unwrap() as u64),
                        WasmType::F32 => Value::F32(args[index].f32().unwrap().to_bits()),
                        WasmType::F64 => Value::F64(args[index].f64().unwrap().to_bits()),
                        WasmType::ExternRef => Value::ExternRef(
                            args[index]
                                .externref()
                                .unwrap()
                                .map(|r| ExternRef::Wasmtime(*r, PhantomData)),
                        ),
                    })
                })
                .unwrap();
                let value = function
                    .call(
                        Caller {
                            context: &mut context,
                        },
                        params,
                    )
                    .map_err(|e| e.at(module, name))?;
                if let Some(value) = R::encode(value) {
                    results[0] = match value {
                        Value::I32(v) => wasmtime::Val::I32(v as i32),
                        Value::I64(v) => wasmtime::Val::I64(v as i64),
                        Value::F32(v) => wasmtime::Val::F32(v),
                        Value::F64(v) => wasmtime::Val::F64(v),
                        Value::ExternRef(None) => wasmtime::Val::ExternRef(None),
                        Value::ExternRef(Some(ExternRef::Wasmtime(r, _))) => {
                            wasmtime::Val::ExternRef(Some(r))
                        }
                        #[cfg(any(feature = "v8", feature = "wee8"))]
                        Value::ExternRef(Some(_)) => unreachable!("reference from another adapter"),
                    };
                }
                Ok(())
            })?;
        Ok(())
    }
}
