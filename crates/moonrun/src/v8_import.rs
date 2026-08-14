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

//! Shared V8 adapter mechanics for memory-consuming wasm host imports.

use std::fmt::Debug;
use std::ptr::NonNull;
use std::sync::OnceLock;

use crate::v8_builder::ObjectExt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum V8ImportError {
    Fault,
    InvalidArgument,
}

/// Memory binding shared by every memory-consuming host import in one run.
///
/// The `WebAssembly.Memory` object is stable. Its backing buffer is not, so an
/// import must reacquire the buffer on every guest-to-host call.
pub(crate) struct V8ImportState {
    memory: OnceLock<v8::Global<v8::WasmMemoryObject>>,
}

impl V8ImportState {
    pub(crate) fn new() -> Self {
        Self {
            memory: OnceLock::new(),
        }
    }

    fn set_memory(
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

fn memory_context<'s>(args: &v8::FunctionCallbackArguments<'s>) -> &'s V8ImportState {
    // SAFETY: `register_memory_setter` installs only a `V8ImportState` pointer
    // and its owner retains that state for the complete V8 run.
    unsafe { callback_context(args) }
}

fn set_memory(
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
        memory_context(&args).set_memory(scope, memory)
    })();
    if let Err(error) = result {
        throw_import_error(scope, "__moonrun_v8_import", "set_memory", error);
    }
}

pub(crate) fn register_memory_setter<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    state: *const V8ImportState,
) {
    register_func(obj, scope, "set_memory", set_memory, state);
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

    pub(crate) fn next_u64(&mut self) -> Result<u64, V8ImportError> {
        let value = self.args.get(self.next_index);
        self.next_index += 1;
        if !value.is_big_int() {
            return Err(V8ImportError::InvalidArgument);
        }
        let bigint =
            v8::Local::<v8::BigInt>::try_from(value).map_err(|_| V8ImportError::InvalidArgument)?;
        let (result, lossless) = bigint.u64_value();
        lossless
            .then_some(result)
            .ok_or(V8ImportError::InvalidArgument)
    }
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
