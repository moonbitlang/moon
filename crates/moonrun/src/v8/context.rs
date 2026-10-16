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

//! Per-run context and shared mechanics for the current V8 host adapters.

use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::OnceLock;

use crate::run_termination::TerminationRequest;
use crate::runtime::Runtime;

use super::builder::ObjectExt;

/// Failure of a synchronous value import. `V8` preserves an exception or
/// termination already raised by an engine operation; it must not enter V8 again.
pub(crate) enum ValueImportError {
    V8,
    Type(&'static str),
    Range(&'static str),
}

/// Complete one synchronous V8 callback. The operation returns ordinary Rust
/// results; only this adapter translates an operation error into a JS exception.
pub(crate) fn value_import<'s>(
    scope: &mut v8::HandleScope<'s>,
    mut ret: v8::ReturnValue,
    operation: impl FnOnce(
        &mut v8::HandleScope<'s>,
    ) -> Result<v8::Local<'s, v8::Value>, ValueImportError>,
) {
    match operation(scope) {
        Ok(value) => ret.set(value),
        Err(ValueImportError::V8) => {}
        Err(ValueImportError::Type(message)) => {
            let message =
                v8::String::new(scope, message).unwrap_or_else(|| v8::String::empty(scope));
            let exception = v8::Exception::type_error(scope, message);
            scope.throw_exception(exception);
        }
        Err(ValueImportError::Range(message)) => {
            let message =
                v8::String::new(scope, message).unwrap_or_else(|| v8::String::empty(scope));
            let exception = v8::Exception::range_error(scope, message);
            scope.throw_exception(exception);
        }
    }
}

pub(crate) fn wasm_constructor<'s>(
    scope: &mut v8::HandleScope<'s>,
    name: &str,
) -> anyhow::Result<v8::Local<'s, v8::Function>> {
    use super::builder::ScopeExt;
    use anyhow::Context;

    let global = scope.get_current_context().global(scope);
    let key = scope.string("WebAssembly");
    let webassembly = global
        .get(scope, key.into())
        .and_then(|value| v8::Local::<v8::Object>::try_from(value).ok())
        .context("V8 did not provide WebAssembly")?;
    let key = scope.string(name);
    webassembly
        .get(scope, key.into())
        .and_then(|value| v8::Local::<v8::Function>::try_from(value).ok())
        .with_context(|| format!("V8 did not provide WebAssembly.{name}"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum V8ImportError {
    Fault,
    InvalidArgument,
}

/// Memory binding shared by every memory-consuming host import in one run.
///
/// The `WebAssembly.Memory` object is stable. Its backing buffer is not, so an
/// import must reacquire the buffer on every guest-to-host call.
pub(crate) struct V8MemoryBinding {
    memory: OnceLock<v8::Global<v8::WasmMemoryObject>>,
}

impl V8MemoryBinding {
    pub(crate) fn new() -> Self {
        Self {
            memory: OnceLock::new(),
        }
    }

    pub(crate) fn bind(
        &self,
        scope: &mut v8::HandleScope,
        memory: v8::Local<v8::WasmMemoryObject>,
    ) -> Result<(), V8ImportError> {
        self.memory
            .set(v8::Global::new(scope, memory))
            .map_err(|_| V8ImportError::InvalidArgument)
    }

    /// Acquire memory for one import while allowing memory-free operations to
    /// proceed when no binding is available. No V8 call may occur in `f`.
    pub(crate) fn with_memory_access<T>(
        &self,
        scope: &mut v8::HandleScope,
        f: impl FnOnce(Result<&mut [u8], V8ImportError>) -> T,
    ) -> T {
        let memory_object = match self.memory_object(scope) {
            Ok(memory) => memory,
            Err(error) => return f(Err(error)),
        };
        let buffer = memory_object.buffer();
        let len = buffer.byte_length();
        let pointer = match buffer.data() {
            Some(pointer) => pointer.cast::<u8>(),
            None if len == 0 => NonNull::dangling(),
            None => return f(Err(V8ImportError::Fault)),
        };
        let memory = unsafe { std::slice::from_raw_parts_mut(pointer.as_ptr(), len) };
        f(Ok(memory))
    }

    fn memory_object<'s>(
        &self,
        scope: &mut v8::HandleScope<'s>,
    ) -> Result<v8::Local<'s, v8::WasmMemoryObject>, V8ImportError> {
        self.memory
            .get()
            .map(|memory| v8::Local::new(scope, memory))
            .ok_or(V8ImportError::Fault)
    }
}

/// Per-run state needed by V8 adapters for backend-neutral Host operations.
///
/// This is deliberately V8-private. Other wasm runtimes acquire Guest Memory
/// and retain the backend-neutral Runtime through their own adapter mechanisms.
pub(crate) struct V8RunContext {
    runtime: Rc<Runtime>,
    memory_binding: Rc<V8MemoryBinding>,
    termination_request: TerminationRequest,
}

impl V8RunContext {
    pub(crate) fn new(runtime: Runtime, termination_request: TerminationRequest) -> Self {
        Self {
            runtime: Rc::new(runtime),
            memory_binding: Rc::new(V8MemoryBinding::new()),
            termination_request,
        }
    }

    pub(crate) fn runtime(&self) -> &Rc<Runtime> {
        &self.runtime
    }

    pub(crate) fn memory_binding(&self) -> &Rc<V8MemoryBinding> {
        &self.memory_binding
    }

    pub(crate) fn termination_request(&self) -> &TerminationRequest {
        &self.termination_request
    }
}

/// Recover the concrete context pointer installed with `register_func`.
///
/// # Safety
///
/// `T` must be the exact type of the pointer passed when this callback was
/// registered, and that pointee must remain alive for the callback lifetime.
pub(crate) unsafe fn callback_context<'s, T>(args: &v8::FunctionCallbackArguments<'s>) -> &'s T {
    let data = args.data();
    assert!(data.is_external());
    let data: v8::Local<v8::Data> = data.into();
    let pointer = v8::Local::<v8::External>::try_from(data).unwrap().value();
    unsafe { &*(pointer as *const T) }
}

pub(crate) fn decode_wasm_u64(value: v8::Local<v8::Value>) -> Option<u64> {
    let bigint = v8::Local::<v8::BigInt>::try_from(value).ok()?;
    let (signed, signed_lossless) = bigint.i64_value();
    if signed_lossless {
        return Some(u64::from_ne_bytes(signed.to_ne_bytes()));
    }
    let (unsigned, unsigned_lossless) = bigint.u64_value();
    unsigned_lossless.then_some(unsigned)
}

pub(crate) fn register_func<'s, T>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
    context: *const T,
) {
    let data = v8::External::new(scope, context as *mut std::ffi::c_void);
    let function = v8::Function::builder(callback)
        .data(data.into())
        .build(scope)
        .unwrap();
    obj.set_value(scope, name, function.into());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasm_u64_decoder_preserves_all_i64_bit_patterns() {
        crate::v8::initialize(&crate::engine::EngineConfig::default()).unwrap();
        let isolate = &mut v8::Isolate::new(Default::default());
        let scope = &mut v8::HandleScope::new(isolate);
        let context = v8::Context::new(scope, Default::default());
        let scope = &mut v8::ContextScope::new(scope, context);

        let signed = v8::BigInt::new_from_i64(scope, -1);
        assert_eq!(decode_wasm_u64(signed.into()), Some(u64::MAX));

        let unsigned = v8::BigInt::new_from_u64(scope, u64::MAX);
        assert_eq!(decode_wasm_u64(unsigned.into()), Some(u64::MAX));
    }
}
