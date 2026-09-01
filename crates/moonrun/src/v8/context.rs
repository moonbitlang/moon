// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Per-run context and shared mechanics for the current V8 host adapters.

use std::fmt::Debug;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::OnceLock;

use crate::run_termination::TerminationRequest;
use crate::runtime::Runtime;

use super::builder::ObjectExt;

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

    fn bind(
        &self,
        scope: &mut v8::HandleScope,
        memory: v8::Local<v8::WasmMemoryObject>,
    ) -> Result<(), V8ImportError> {
        self.memory
            .set(v8::Global::new(scope, memory))
            .map_err(|_| V8ImportError::InvalidArgument)
    }

    pub(crate) fn with_memory_mut<T, E>(
        &self,
        scope: &mut v8::HandleScope,
        f: impl FnOnce(&mut [u8]) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<V8ImportError>,
    {
        let memory_object = self.memory_object(scope)?;
        let buffer = memory_object.buffer();
        let len = buffer.byte_length();
        let pointer = match buffer.data() {
            Some(pointer) => pointer.cast::<u8>(),
            None if len == 0 => NonNull::dangling(),
            None => return Err(V8ImportError::Fault.into()),
        };
        let memory = unsafe { std::slice::from_raw_parts_mut(pointer.as_ptr(), len) };
        f(memory)
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
    runtime: Runtime,
    memory_binding: Rc<V8MemoryBinding>,
    termination_request: TerminationRequest,
}

impl V8RunContext {
    pub(crate) fn new(runtime: Runtime, termination_request: TerminationRequest) -> Self {
        Self {
            runtime,
            memory_binding: Rc::new(V8MemoryBinding::new()),
            termination_request,
        }
    }

    pub(crate) fn runtime(&self) -> &Runtime {
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

fn memory_binding<'s>(args: &v8::FunctionCallbackArguments<'s>) -> &'s V8MemoryBinding {
    // SAFETY: `register_memory_binder` installs only a `V8MemoryBinding`
    // pointer and its owner retains that binding for the complete V8 run.
    unsafe { callback_context(args) }
}

fn bind_memory(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _ret: v8::ReturnValue,
) {
    let result = (|| {
        if args.length() != 1 {
            return Err(V8ImportError::InvalidArgument);
        }
        let memory = v8::Local::<v8::WasmMemoryObject>::try_from(args.get(0))
            .map_err(|_| V8ImportError::InvalidArgument)?;
        memory_binding(&args).bind(scope, memory)
    })();
    if let Err(error) = result {
        throw_import_error(scope, "__moonrun_v8_import", "bind_memory", error);
    }
}

/// Register the bootstrap hook that binds the instance's exported memory.
///
/// The JS runner can obtain an exported memory only after instantiation. Until
/// it calls this hook, memory-consuming imports fail with `Fault`.
/// # Safety
///
/// `binding` must remain valid whenever the registered callback can be
/// invoked.
pub(crate) unsafe fn register_memory_binder<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    binding: *const V8MemoryBinding,
) {
    register_func(obj, scope, "bind_memory", bind_memory, binding);
}

pub(crate) struct ImportArgs<'a, 'scope, 'args> {
    scope: &'a mut v8::HandleScope<'scope>,
    args: &'a v8::FunctionCallbackArguments<'args>,
    next_index: i32,
}

impl<'a, 'scope, 'args> ImportArgs<'a, 'scope, 'args> {
    pub(crate) fn new(
        scope: &'a mut v8::HandleScope<'scope>,
        args: &'a v8::FunctionCallbackArguments<'args>,
    ) -> Self {
        Self {
            scope,
            args,
            next_index: 0,
        }
    }

    pub(crate) fn next_i32(&mut self) -> Result<i32, V8ImportError> {
        let value = self.args.get(self.next_index);
        self.next_index += 1;
        value
            .int32_value(self.scope)
            .ok_or(V8ImportError::InvalidArgument)
    }

    /// Decode the same Wasm `i32` value with unsigned Rust semantics.
    pub(crate) fn next_u32(&mut self) -> Result<u32, V8ImportError> {
        let value = self.args.get(self.next_index);
        self.next_index += 1;
        value
            .uint32_value(self.scope)
            .ok_or(V8ImportError::InvalidArgument)
    }

    pub(crate) fn next_i64(&mut self) -> Result<i64, V8ImportError> {
        let value = self.args.get(self.next_index);
        self.next_index += 1;
        if value.is_big_int() {
            let bigint = v8::Local::<v8::BigInt>::try_from(value)
                .map_err(|_| V8ImportError::InvalidArgument)?;
            let (result, lossless) = bigint.i64_value();
            if lossless {
                return Ok(result);
            }
        }
        value
            .integer_value(self.scope)
            .ok_or(V8ImportError::InvalidArgument)
    }

    pub(crate) fn next_f64(&mut self) -> Result<f64, V8ImportError> {
        let value = self.args.get(self.next_index);
        self.next_index += 1;
        value
            .number_value(self.scope)
            .ok_or(V8ImportError::InvalidArgument)
    }

    pub(crate) fn next_u64(&mut self) -> Result<u64, V8ImportError> {
        let value = self.args.get(self.next_index);
        self.next_index += 1;
        decode_wasm_u64(value).ok_or(V8ImportError::InvalidArgument)
    }
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

pub(crate) fn throw_import_error(
    scope: &mut v8::HandleScope,
    module: &str,
    import_name: &str,
    error: impl Debug,
) {
    let message = format!("{module}.{import_name} failed: {error:?}");
    let message = v8::String::new(scope, &message).unwrap_or_else(|| v8::String::empty(scope));
    let exception = v8::Exception::error(scope, message);
    scope.throw_exception(exception);
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
