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

//! V8 callback ownership and conversion for shared host imports.

use super::{
    builder::ScopeExt,
    context::{V8MemoryBinding, decode_wasm_u64},
};
use crate::wasm_imports::{
    CallContext, Caller, ExternRef, HostFunction, ImportError, ImportRegistry, MemoryCaller, Value,
    WasmParams, WasmResult, WasmType,
};
use std::{any::Any, convert::Infallible, rc::Rc};

pub(super) struct Registry<'a, 's, T> {
    pub(super) object: v8::Local<'s, v8::Object>,
    pub(super) scope: &'a mut v8::HandleScope<'s>,
    pub(super) data: Rc<T>,
    pub(super) memory: Rc<V8MemoryBinding>,
    pub(super) termination: crate::run_termination::TerminationRequest,
    pub(super) values: Rc<super::values::State>,
    pub(super) retained: &'a mut Vec<Box<dyn Any>>,
}

struct Binding<T, F> {
    data: Rc<T>,
    memory: Rc<V8MemoryBinding>,
    function: F,
    values: Rc<super::values::State>,
    termination: crate::run_termination::TerminationRequest,
    module: &'static str,
    name: &'static str,
}

impl<T: 'static> ImportRegistry<T> for Registry<'_, '_, T> {
    type Error = Infallible;

    fn func_wrap<P: WasmParams, R: WasmResult, F: HostFunction<T, P, R>>(
        &mut self,
        module: &'static str,
        name: &'static str,
        function: F,
    ) -> Result<(), Infallible> {
        let key = self.scope.string(module);
        let namespace = self.object.get(self.scope, key.into()).unwrap();
        let namespace = v8::Local::<v8::Object>::try_from(namespace).unwrap_or_else(|_| {
            let namespace = v8::Object::new(self.scope);
            self.object.set(self.scope, key.into(), namespace.into());
            namespace
        });
        let binding = Box::new(Binding {
            data: Rc::clone(&self.data),
            memory: Rc::clone(&self.memory),
            function,
            values: Rc::clone(&self.values),
            termination: self.termination.clone(),
            module,
            name,
        });
        let external = v8::External::new(
            self.scope,
            (&*binding as *const Binding<T, F>).cast_mut().cast(),
        );
        let callback = v8::Function::builder(invoke::<T, P, R, F>)
            .length(P::TYPES.len() as i32)
            .data(external.into())
            .build(self.scope)
            .unwrap();
        let key = self.scope.string(name);
        namespace.set(self.scope, key.into(), callback.into());
        self.retained.push(binding);
        Ok(())
    }
}

pub(super) struct Context<'a, 's, T> {
    pub(super) scope: &'a mut v8::HandleScope<'s>,
    pub(super) values: &'a super::values::State,
    data: &'a T,
    memory: &'a V8MemoryBinding,
    termination: &'a crate::run_termination::TerminationRequest,
}
impl<'s, T> CallContext<'s, T> for Context<'_, 's, T> {
    fn with_data(&self, operation: &mut dyn FnMut(&T)) {
        operation(self.data);
    }
    fn with_memory(&mut self, operation: &mut dyn FnMut(MemoryCaller<'_, T>)) {
        self.memory.with_memory_access(self.scope, |memory| {
            operation(MemoryCaller {
                data: self.data,
                memory: memory.map_err(|_| ImportError::MemoryUnavailable),
                termination: self.termination,
            })
        });
    }
    fn terminate(&self, outcome: crate::run_termination::RunTermination) {
        self.termination.request(outcome);
    }
}
fn invoke<'s, T, P: WasmParams, R: WasmResult, F: HostFunction<T, P, R>>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut ret: v8::ReturnValue,
) {
    let Ok(_termination) = super::context::enter_host_call(scope) else {
        return;
    };
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    // Registry retains this exact allocation until the run's callbacks are gone.
    let binding = unsafe { &*external.value().cast::<Binding<T, F>>() };
    let Ok(params) = P::decode(|index, kind| {
        let value = args.get(index as i32);
        match kind {
            WasmType::I32 => value
                .int32_value(scope)
                .map(|value| Value::I32(value as u32)),
            // Preserve the complete i64 bit pattern across V8's BigInt boundary.
            WasmType::I64 => decode_wasm_u64(value).map(Value::I64),
            WasmType::F32 => value
                .number_value(scope)
                .map(|value| Value::F32((value as f32).to_bits())),
            WasmType::F64 => value
                .number_value(scope)
                .map(|value| Value::F64(value.to_bits())),
            WasmType::ExternRef => Some(Value::ExternRef(
                (!value.is_null()).then_some(ExternRef::V8(value)),
            )),
        }
        .ok_or(())
    }) else {
        let message = scope.string(&format!(
            "invalid arguments for {}.{}",
            binding.module, binding.name
        ));
        let exception = v8::Exception::type_error(scope, message);
        scope.throw_exception(exception);
        return;
    };
    let mut context = Context {
        scope,
        values: &binding.values,
        data: binding.data.as_ref(),
        memory: &binding.memory,
        termination: &binding.termination,
    };
    let result = binding.function.call(
        Caller {
            context: &mut context,
        },
        params,
    );
    match result.map_err(|error| error.at(binding.module, binding.name)) {
        Ok(value) => match R::encode(value) {
            None => ret.set_undefined(),
            Some(Value::I32(value)) => ret.set_int32(value as i32),
            Some(Value::I64(value)) => ret.set(v8::BigInt::new_from_u64(scope, value).into()),
            Some(Value::F32(value)) => ret.set_double(f32::from_bits(value) as f64),
            Some(Value::F64(value)) => ret.set_double(f64::from_bits(value)),
            Some(Value::ExternRef(None)) => ret.set_null(),
            Some(Value::ExternRef(Some(ExternRef::V8(value)))) => ret.set(value),
        },
        Err(ImportError::Terminated) => {
            scope.terminate_execution();
        }
        Err(ImportError::V8(super::context::ValueImportError::V8)) => {}
        Err(ImportError::V8(super::context::ValueImportError::Type(message))) => {
            let message = scope.string(message);
            let exception = v8::Exception::type_error(scope, message);
            scope.throw_exception(exception);
        }
        Err(ImportError::V8(super::context::ValueImportError::Range(message))) => {
            let message = scope.string(message);
            let exception = v8::Exception::range_error(scope, message);
            scope.throw_exception(exception);
        }
        Err(error) => {
            let message = scope.string(&error.to_string());
            let exception = v8::Exception::error(scope, message);
            scope.throw_exception(exception);
        }
    }
}
