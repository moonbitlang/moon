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

use std::ptr::NonNull;
use std::sync::OnceLock;

use crate::async_host::{AsyncHost, AsyncHostError, AsyncHostResult};

pub(super) struct AsyncContext {
    pub(super) host: AsyncHost,
    imports: v8::Global<v8::Object>,
    // The memory object is stable, but memory.grow may replace its buffer.
    // Cache the object while reacquiring buffer storage for every import.
    memory: OnceLock<v8::Global<v8::WasmMemoryObject>>,
}

impl AsyncContext {
    pub(super) fn new<'s>(
        scope: &mut v8::HandleScope<'s>,
        imports: v8::Local<'s, v8::Object>,
        host: AsyncHost,
    ) -> Self {
        Self {
            host,
            imports: v8::Global::new(scope, imports),
            memory: OnceLock::new(),
        }
    }
}

impl Drop for AsyncContext {
    fn drop(&mut self) {
        self.host.assert_no_leaked_handles_if_enabled();
    }
}

pub(super) fn callback_context<'s>(args: &v8::FunctionCallbackArguments<'s>) -> &'s AsyncContext {
    let data = args.data();
    assert!(data.is_external());
    let data: v8::Local<v8::Data> = data.into();
    let ptr = v8::Local::<v8::External>::try_from(data).unwrap().value();
    unsafe { &*(ptr as *const AsyncContext) }
}

pub(super) struct ImportContext<'a, 'scope> {
    pub(super) scope: &'a mut v8::HandleScope<'scope>,
    pub(super) host: &'a AsyncHost,
    imports: &'a v8::Global<v8::Object>,
    memory: &'a OnceLock<v8::Global<v8::WasmMemoryObject>>,
}

impl<'a, 'scope> ImportContext<'a, 'scope> {
    pub(super) fn new(scope: &'a mut v8::HandleScope<'scope>, context: &'a AsyncContext) -> Self {
        Self {
            scope,
            host: &context.host,
            imports: &context.imports,
            memory: &context.memory,
        }
    }

    pub(super) fn with_host_and_memory_mut<T>(
        &mut self,
        f: impl FnOnce(&AsyncHost, &mut [u8]) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let host = self.host;
        let memory_object = memory_object(self.scope, self.imports, self.memory)?;
        let buffer = memory_object.buffer();
        let len = buffer.byte_length();

        let ptr = match buffer.data() {
            Some(ptr) => ptr.cast::<u8>(),
            None if len == 0 => NonNull::dangling(),
            None => return Err(AsyncHostError::Fault),
        };

        let memory = unsafe { std::slice::from_raw_parts_mut(ptr.as_ptr(), len) };
        f(host, memory)
    }

    pub(super) fn with_memory_mut<T>(
        &mut self,
        f: impl FnOnce(&mut [u8]) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        self.with_host_and_memory_mut(|_, memory| f(memory))
    }
}

pub(super) struct ImportArgs<'a, 'scope, 'args> {
    scope: &'a mut v8::HandleScope<'scope>,
    args: &'a v8::FunctionCallbackArguments<'args>,
    next_index: i32,
}

impl<'a, 'scope, 'args> ImportArgs<'a, 'scope, 'args> {
    pub(super) fn new(
        scope: &'a mut v8::HandleScope<'scope>,
        args: &'a v8::FunctionCallbackArguments<'args>,
    ) -> Self {
        Self {
            scope,
            args,
            next_index: 0,
        }
    }

    pub(super) fn next_i32(&mut self) -> AsyncHostResult<i32> {
        let index = self.next_index;
        self.next_index += 1;
        self.args
            .get(index)
            .int32_value(self.scope)
            .ok_or(AsyncHostError::Inval)
    }

    pub(super) fn next_i64(&mut self) -> AsyncHostResult<i64> {
        let index = self.next_index;
        self.next_index += 1;
        let value = self.args.get(index);
        if value.is_big_int() {
            let bigint =
                v8::Local::<v8::BigInt>::try_from(value).map_err(|_| AsyncHostError::Inval)?;
            let (result, lossless) = bigint.i64_value();
            if lossless {
                return Ok(result);
            }
        }
        value.integer_value(self.scope).ok_or(AsyncHostError::Inval)
    }

    pub(super) fn next_u64(&mut self) -> AsyncHostResult<u64> {
        let index = self.next_index;
        self.next_index += 1;
        let value = self.args.get(index);
        if !value.is_big_int() {
            return Err(AsyncHostError::Inval);
        }
        let bigint = v8::Local::<v8::BigInt>::try_from(value).map_err(|_| AsyncHostError::Inval)?;
        let (result, lossless) = bigint.u64_value();
        if lossless {
            Ok(result)
        } else {
            Err(AsyncHostError::Inval)
        }
    }
}

fn memory_object<'s>(
    scope: &mut v8::HandleScope<'s>,
    imports: &v8::Global<v8::Object>,
    cached: &OnceLock<v8::Global<v8::WasmMemoryObject>>,
) -> AsyncHostResult<v8::Local<'s, v8::WasmMemoryObject>> {
    if let Some(memory) = cached.get() {
        return Ok(v8::Local::new(scope, memory));
    }

    let imports = v8::Local::new(scope, imports);
    let key = v8::String::new(scope, "memory").ok_or(AsyncHostError::Fault)?;
    let memory = imports
        .get(scope, key.into())
        .ok_or(AsyncHostError::Fault)?;
    let memory =
        v8::Local::<v8::WasmMemoryObject>::try_from(memory).map_err(|_| AsyncHostError::Fault)?;
    let _ = cached.set(v8::Global::new(scope, memory));
    Ok(memory)
}

pub(super) fn throw_import_error(
    scope: &mut v8::HandleScope,
    import_name: &str,
    error: AsyncHostError,
) {
    let message = format!("moonbitlang/async.{import_name} failed: {error:?}");
    let message = v8::String::new(scope, &message).unwrap_or_else(|| v8::String::empty(scope));
    let exception = v8::Exception::error(scope, message);
    scope.throw_exception(exception);
}

pub(super) trait FinishVoid {
    fn finish_void(self, scope: &mut v8::HandleScope, ret: &mut v8::ReturnValue, import_name: &str);
}

impl FinishVoid for () {
    fn finish_void(
        self,
        _scope: &mut v8::HandleScope,
        ret: &mut v8::ReturnValue,
        _import_name: &str,
    ) {
        ret.set_undefined();
    }
}

impl FinishVoid for AsyncHostResult<()> {
    fn finish_void(
        self,
        scope: &mut v8::HandleScope,
        ret: &mut v8::ReturnValue,
        import_name: &str,
    ) {
        match self {
            Ok(()) => ret.set_undefined(),
            Err(error) => throw_import_error(scope, import_name, error),
        }
    }
}

pub(super) trait FinishI32 {
    fn finish_i32(self, scope: &mut v8::HandleScope, ret: &mut v8::ReturnValue, import_name: &str);
}

impl FinishI32 for i32 {
    fn finish_i32(
        self,
        _scope: &mut v8::HandleScope,
        ret: &mut v8::ReturnValue,
        _import_name: &str,
    ) {
        ret.set_int32(self);
    }
}

impl FinishI32 for AsyncHostResult<i32> {
    fn finish_i32(self, scope: &mut v8::HandleScope, ret: &mut v8::ReturnValue, import_name: &str) {
        match self {
            Ok(value) => ret.set_int32(value),
            Err(error) => throw_import_error(scope, import_name, error),
        }
    }
}

pub(super) trait FinishI64 {
    fn finish_i64(self, scope: &mut v8::HandleScope, ret: &mut v8::ReturnValue, import_name: &str);
}

impl FinishI64 for i64 {
    fn finish_i64(
        self,
        scope: &mut v8::HandleScope,
        ret: &mut v8::ReturnValue,
        _import_name: &str,
    ) {
        ret.set(v8::BigInt::new_from_i64(scope, self).into());
    }
}

impl FinishI64 for u64 {
    fn finish_i64(
        self,
        scope: &mut v8::HandleScope,
        ret: &mut v8::ReturnValue,
        _import_name: &str,
    ) {
        ret.set(v8::BigInt::new_from_u64(scope, self).into());
    }
}

impl FinishI64 for AsyncHostResult<i64> {
    fn finish_i64(self, scope: &mut v8::HandleScope, ret: &mut v8::ReturnValue, import_name: &str) {
        match self {
            Ok(value) => value.finish_i64(scope, ret, import_name),
            Err(error) => throw_import_error(scope, import_name, error),
        }
    }
}

impl FinishI64 for AsyncHostResult<u64> {
    fn finish_i64(self, scope: &mut v8::HandleScope, ret: &mut v8::ReturnValue, import_name: &str) {
        match self {
            Ok(value) => value.finish_i64(scope, ret, import_name),
            Err(error) => throw_import_error(scope, import_name, error),
        }
    }
}
