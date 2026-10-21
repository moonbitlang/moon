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

//! Bind shared typed definitions in the module's import order.
use super::{StoreData, api};
use crate::wasm_imports::{
    CallContext, Caller, ExternRef, HostFunction, ImportError, ImportRegistry, MemoryCaller, Value,
    WasmParams, WasmResult, WasmType, payload::PayloadContext,
};
use std::{any::Any, marker::PhantomData, rc::Rc};

pub(super) struct Registry<'s> {
    store: &'s api::Store<StoreData>,
    imports: Vec<api::Import>,
    functions: Vec<Option<api::HostFunc<'s, StoreData>>>,
}
impl<'s> Registry<'s> {
    pub(super) fn new(
        store: &'s api::Store<StoreData>,
        module: &api::Module<'s, StoreData>,
    ) -> api::Result<Self> {
        let imports = module.imports()?;
        let functions = (0..imports.len()).map(|_| None).collect();
        Ok(Self {
            store,
            imports,
            functions,
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
struct Context<'s, T> {
    caller: api::Caller<'s, StoreData>,
    marker: PhantomData<T>,
}
impl<'call, T> PayloadContext<'call> for Context<'call, T> {
    fn string_units(&self, value: &Option<ExternRef<'call>>) -> Result<Vec<u16>, ImportError> {
        let Some(ExternRef::Wee8(value)) = value else {
            return Err(ImportError::Operation(
                "externref is not a JS string".into(),
            ));
        };
        api::StringRef::try_from(value.clone())
            .map(|string| string.to_utf16())
            .map_err(|_| ImportError::Operation("externref is not a JS string".into()))
    }
    fn new_string_units(&mut self, units: &[u16]) -> Result<Option<ExternRef<'call>>, ImportError> {
        api::StringRef::from_utf16(self.caller.as_context(), units)
            .map(|string| Some(ExternRef::Wee8(string.into_extern_ref())))
            .map_err(|e| ImportError::Host(e.into()))
    }
    fn new_string_utf8(&mut self, text: &str) -> Result<Option<ExternRef<'call>>, ImportError> {
        api::StringRef::from_utf8(self.caller.as_context(), text)
            .map(|string| Some(ExternRef::Wee8(string.into_extern_ref())))
            .map_err(|e| ImportError::Host(e.into()))
    }
    fn string_utf8_lossy(&self, value: &Option<ExternRef<'call>>) -> Result<String, ImportError> {
        let Some(ExternRef::Wee8(value)) = value else {
            return Err(ImportError::Operation(
                "externref is not a JS string".into(),
            ));
        };
        api::StringRef::try_from(value.clone())
            .map(|string| string.to_string_lossy())
            .map_err(|_| ImportError::Operation("externref is not a JS string".into()))
    }
    fn is_string_value(&self, value: &Option<ExternRef<'call>>) -> Result<bool, ImportError> {
        match value {
            None => Ok(false),
            Some(ExternRef::Wee8(value)) => Ok(value.is_string()),
            #[cfg(feature = "wasmtime")]
            _ => Err(ImportError::Operation(
                "reference from another engine".into(),
            )),
        }
    }
    fn new_payload(
        &mut self,
        data: Box<dyn Any + Send + Sync>,
    ) -> Result<Option<ExternRef<'call>>, ImportError> {
        // Store the trait object itself, not a second nested box/type registry.
        self.caller
            .extern_ref(data)
            .map(|r| Some(ExternRef::Wee8(r)))
            .map_err(|e| ImportError::Host(e.into()))
    }
    fn with_payload(
        &self,
        value: &Option<ExternRef<'call>>,
        operation: &mut dyn FnMut(Option<&dyn Any>),
    ) -> Result<(), ImportError> {
        let Some(ExternRef::Wee8(value)) = value else {
            return Err(ImportError::Operation("invalid host reference".into()));
        };
        operation(
            value
                .data::<Box<dyn Any + Send + Sync>>()
                .map(|data| data.as_ref() as &dyn Any),
        );
        Ok(())
    }
}
impl<'call, T> CallContext<'call, T> for Context<'call, T>
where
    StoreData: AsRef<T>,
{
    fn with_data(&self, operation: &mut dyn FnMut(&T)) {
        self.caller.with_data(|s| operation(s.as_ref()));
    }
    fn with_memory(&mut self, operation: &mut dyn FnMut(MemoryCaller<'_, T>)) {
        if let Err(error) = self.caller.with_memory(|s, memory| {
            operation(MemoryCaller {
                data: s.as_ref(),
                memory: Ok(memory),
                termination: &s.termination,
            })
        }) {
            self.caller.with_data(|s| {
                operation(MemoryCaller {
                    data: s.as_ref(),
                    memory: Err(ImportError::Host(error.into())),
                    termination: &s.termination,
                })
            });
        }
    }
    fn terminate(&self, outcome: crate::run_termination::RunTermination) {
        self.caller.with_data(|s| s.termination.request(outcome));
    }
}
impl<T: 'static> ImportRegistry<T> for Registry<'_>
where
    StoreData: AsRef<T>,
{
    type Error = api::Error;
    fn func_wrap<P: WasmParams, R: WasmResult, F: HostFunction<T, P, R>>(
        &mut self,
        module: &'static str,
        name: &'static str,
        function: F,
    ) -> api::Result<()> {
        let function = Rc::new(function);
        let kind = |kind| match kind {
            WasmType::I32 => api::Kind::I32,
            WasmType::I64 => api::Kind::I64,
            WasmType::F32 => api::Kind::F32,
            WasmType::F64 => api::Kind::F64,
            WasmType::ExternRef => api::Kind::ExternRef,
        };
        let params: Vec<_> = P::TYPES.iter().copied().map(kind).collect();
        let results: Vec<_> = R::TYPE.map(kind).into_iter().collect();
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
                        let mut context = Context::<T> {
                            caller,
                            marker: PhantomData,
                        };
                        let params = P::decode::<std::convert::Infallible>(|index, _| {
                            Ok(match &args[index] {
                                api::Value::I32(v) => Value::I32(*v as u32),
                                api::Value::I64(v) => Value::I64(*v as u64),
                                api::Value::F32(v) => Value::F32(v.to_bits()),
                                api::Value::F64(v) => Value::F64(v.to_bits()),
                                api::Value::ExternRef(v) => {
                                    Value::ExternRef(v.clone().map(ExternRef::Wee8))
                                }
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
                            .map_err(|e| api::Error::Host(e.at(module, name).to_string()))?;
                        Ok(R::encode(value)
                            .map(|value| match value {
                                Value::I32(v) => api::Value::I32(v as i32),
                                Value::I64(v) => api::Value::I64(v as i64),
                                Value::F32(v) => api::Value::F32(f32::from_bits(v)),
                                Value::F64(v) => api::Value::F64(f64::from_bits(v)),
                                Value::ExternRef(None) => api::Value::ExternRef(None),
                                Value::ExternRef(Some(ExternRef::Wee8(v))) => {
                                    api::Value::ExternRef(Some(v))
                                }
                                #[cfg(feature = "wasmtime")]
                                Value::ExternRef(Some(_)) => {
                                    unreachable!("reference from another adapter")
                                }
                            })
                            .into_iter()
                            .collect())
                    })?,
            );
        }
        Ok(())
    }
}
