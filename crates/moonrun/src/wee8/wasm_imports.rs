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
    pub(super) store: &'s api::Store<StoreData>,
    imports: Vec<api::ImportType>,
    definitions: Vec<Option<Definition<'s>>>,
}
enum Definition<'s> {
    Func(api::HostFunc<'s, StoreData>),
    Tag(api::Tag),
    Memory(api::Memory),
}

impl<'s> Registry<'s> {
    pub(super) fn new(
        store: &'s api::Store<StoreData>,
        module: &api::Module<'s, StoreData>,
    ) -> api::Result<Self> {
        let imports = module.import_types()?;
        let definitions = (0..imports.len()).map(|_| None).collect();
        Ok(Self {
            store,
            imports,
            definitions,
        })
    }
    pub(super) fn register_exceptions(&mut self) -> api::Result<()> {
        // Resolve the native tag first, independently of import order. The throw
        // callback must use this exact identity so the guest can catch it.
        let mut tag = None;
        for (import, slot) in self.imports.iter().zip(&mut self.definitions) {
            if import.module != "exception" || import.name != "tag" {
                continue;
            }
            if !matches!(&import.ty, api::ExternType::Tag { params } if params.is_empty()) {
                return Err("incorrect signature for exception::tag".into());
            }
            if tag.is_some() {
                return Err("duplicate `exception.tag` import".into());
            }
            let native_tag = api::Tag::new(self.store.as_context(), &[])?;
            *slot = Some(Definition::Tag(native_tag));
            tag = Some(native_tag);
        }
        for (import, slot) in self.imports.iter().zip(&mut self.definitions) {
            if import.module != "exception" || import.name != "throw" {
                continue;
            }
            if !matches!(&import.ty, api::ExternType::Func { params, results } if params.is_empty() && results.is_empty())
            {
                return Err("incorrect signature for exception::throw".into());
            }
            let tag = tag.ok_or("missing `exception.tag` import")?;
            // Keep guest exceptions distinct from ImportError::Host: only the
            // native throw path produces a catchable Wasm exception.
            *slot = Some(Definition::Func(self.store.host_func(
                &[],
                &[],
                move |caller, _| {
                    let exception = api::Exception::new(caller.as_context(), tag, &[])?;
                    Err(caller.throw(&exception))
                },
            )?));
        }
        Ok(())
    }

    pub(super) fn define_memory(
        &mut self,
        module: &str,
        name: &str,
        memory: api::Memory,
    ) -> api::Result<()> {
        for (import, slot) in self.imports.iter().zip(&mut self.definitions) {
            if import.module == module && import.name == name {
                if !matches!(import.ty, api::ExternType::Memory(_)) {
                    return Err(format!("incorrect type for {module}::{name}").into());
                }
                *slot = Some(Definition::Memory(memory));
            }
        }
        Ok(())
    }

    pub(super) fn needs_import(&self, module: &str, name: &str) -> bool {
        self.imports
            .iter()
            .any(|import| import.module == module && import.name == name)
    }

    pub(super) fn define_func(
        &mut self,
        module: &str,
        name: &str,
        params: &[api::ValType],
        results: &[api::ValType],
        callback: impl for<'a> Fn(
            api::Caller<'a, StoreData>,
            &[api::Value<'a>],
        ) -> api::Result<Vec<api::Value<'a>>>
        + 'static,
    ) -> api::Result<()> {
        let callback = Rc::new(callback);
        for (import, slot) in self.imports.iter().zip(&mut self.definitions) {
            if import.module != module || import.name != name {
                continue;
            }
            let api::ExternType::Func {
                params: actual_params,
                results: actual_results,
            } = &import.ty
            else {
                return Err(format!("incorrect type for {module}::{name}").into());
            };
            // Shared Rust callbacks use Option<ExternRef> for both signatures.
            // Check that guest types fit that representation, not that the
            // callback's declared signature is a function subtype. Keep the
            // guest's exact signature in V8; wee8 checks non-null results before
            // returning them, rather than weakening the import.
            let represented = |actual: &[api::ValType], declared: &[api::ValType]| {
                actual.len() == declared.len()
                    && actual.iter().zip(declared).all(|(a, d)| a.matches(d))
            };
            if !represented(actual_params, params) || !represented(actual_results, results) {
                return Err(format!("incorrect signature for {module}::{name}").into());
            }
            let callback = callback.clone();
            *slot = Some(Definition::Func(self.store.host_func(
                actual_params,
                actual_results,
                move |caller, args| callback(caller, args),
            )?));
        }
        Ok(())
    }

    pub(super) fn finish(&self) -> api::Result<Vec<api::Extern<'_, StoreData>>> {
        self.imports
            .iter()
            .zip(&self.definitions)
            .map(|(import, definition)| match definition {
                Some(Definition::Func(function)) => Ok(api::Extern::Func(function)),
                Some(Definition::Tag(tag)) => Ok(api::Extern::Tag(*tag)),
                Some(Definition::Memory(memory)) => Ok(api::Extern::Memory(*memory)),
                None => Err(api::Error::Interface(format!(
                    "unsupported wee8 import {}::{}",
                    import.module, import.name
                ))),
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
                "invalid host reference: expected native string".into(),
            ));
        };
        api::StringRef::try_from(value.clone())
            .map(|string| string.to_utf16())
            .map_err(|_| {
                ImportError::Operation("invalid host reference: expected native string".into())
            })
    }
    fn new_string_units(
        &mut self,
        units: Vec<u16>,
    ) -> Result<Option<ExternRef<'call>>, ImportError> {
        api::StringRef::from_utf16(self.caller.as_context(), &units)
            .map(|string| Some(ExternRef::Wee8(string.into_extern_ref())))
            .map_err(|error| ImportError::Host(error.into()))
    }
    fn is_string_value(&self, value: &Option<ExternRef<'call>>) -> Result<bool, ImportError> {
        Ok(
            matches!(value, Some(ExternRef::Wee8(value)) if api::StringRef::try_from(value.clone()).is_ok()),
        )
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
        let val_type = |ty| match ty {
            WasmType::I32 => api::ValType::I32,
            WasmType::I64 => api::ValType::I64,
            WasmType::F32 => api::ValType::F32,
            WasmType::F64 => api::ValType::F64,
            WasmType::ExternRef => api::ValType::EXTERNREF,
        };
        let params: Vec<_> = P::TYPES.iter().copied().map(val_type).collect();
        let results: Vec<_> = R::TYPE.map(val_type).into_iter().collect();
        self.define_func(module, name, &params, &results, move |caller, args| {
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
                    api::Value::ExternRef(v) => Value::ExternRef(v.clone().map(ExternRef::Wee8)),
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
                .map_err(|e| api::Error::Host(e.at(module, name).to_string().into()))?;
            Ok(R::encode(value)
                .map(|value| match value {
                    Value::I32(v) => api::Value::I32(v as i32),
                    Value::I64(v) => api::Value::I64(v as i64),
                    Value::F32(v) => api::Value::F32(f32::from_bits(v)),
                    Value::F64(v) => api::Value::F64(f64::from_bits(v)),
                    Value::ExternRef(None) => api::Value::ExternRef(None),
                    Value::ExternRef(Some(ExternRef::Wee8(v))) => api::Value::ExternRef(Some(v)),
                    #[cfg(feature = "wasmtime")]
                    Value::ExternRef(Some(_)) => {
                        unreachable!("reference from another adapter")
                    }
                })
                .into_iter()
                .collect())
        })
    }
}
