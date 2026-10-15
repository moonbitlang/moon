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
    Caller, HostFunction, ImportError, ImportRegistry, WasmParams, WasmResult, WasmType,
};
use std::{any::Any, convert::Infallible, rc::Rc};

pub(super) struct Registry<'a, 's, T> {
    pub(super) object: v8::Local<'s, v8::Object>,
    pub(super) scope: &'a mut v8::HandleScope<'s>,
    pub(super) module: &'static str,
    pub(super) data: Rc<T>,
    pub(super) memory: Rc<V8MemoryBinding>,
    pub(super) retained: &'a mut Vec<Box<dyn Any>>,
}

struct Binding<T, P, R> {
    data: Rc<T>,
    memory: Rc<V8MemoryBinding>,
    function: HostFunction<T, P, R>,
}

impl<T: 'static> ImportRegistry<T> for Registry<'_, '_, T> {
    type Error = Infallible;

    fn define<P: WasmParams, R: WasmResult>(
        &mut self,
        module: &'static str,
        name: &'static str,
        function: HostFunction<T, P, R>,
    ) -> Result<(), Infallible> {
        // V8 receives the already-created namespace object from import setup.
        assert_eq!(module, self.module);
        let binding = Box::new(Binding {
            data: Rc::clone(&self.data),
            memory: Rc::clone(&self.memory),
            function,
        });
        let external = v8::External::new(
            self.scope,
            (&*binding as *const Binding<T, P, R>).cast_mut().cast(),
        );
        let callback = v8::Function::builder(invoke::<T, P, R>)
            .length(P::TYPES.len() as i32)
            .data(external.into())
            .build(self.scope)
            .unwrap();
        let key = self.scope.string(name);
        self.object.set(self.scope, key.into(), callback.into());
        self.retained.push(binding);
        Ok(())
    }
}

fn invoke<T, P: WasmParams, R: WasmResult>(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    // Registry retains this exact allocation until the run's callbacks are gone.
    let binding = unsafe { &*external.value().cast::<Binding<T, P, R>>() };
    let Ok(params) = P::decode(|index, kind| {
        let value = args.get(index as i32);
        match kind {
            WasmType::I32 => value.int32_value(scope).map(i64::from),
            // Wasm i64 reaches a JS callback as BigInt. Preserve the complete
            // bit pattern, including unsigned rights/cookies with bit 63 set.
            WasmType::I64 => decode_wasm_u64(value).map(|bits| bits as i64),
        }
        .ok_or(())
    }) else {
        return;
    };
    // All conversions precede the memory borrow. No V8 calls occur within it.
    let result = binding.memory.with_memory_access(scope, |memory| {
        (binding.function)(
            Caller {
                data: &binding.data,
                memory: memory.map_err(|_| ImportError::MemoryUnavailable),
            },
            params,
        )
    });
    match result {
        Ok(value) => {
            if R::HAS_RESULT {
                ret.set_int32(value.into_i32().expect("declared i32 result"));
            }
        }
        Err(ImportError::Terminated) => {
            scope.terminate_execution();
        }
        Err(error) => {
            let message = scope.string(&error.to_string());
            let exception = v8::Exception::error(scope, message);
            scope.throw_exception(exception);
        }
    }
}
