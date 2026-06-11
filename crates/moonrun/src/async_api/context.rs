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

use crate::async_host::{AsyncHost, AsyncHostError, AsyncHostResult};

const ASYNC_ERRNO_SUCCESS: i32 = 0;

pub(super) struct AsyncContext {
    pub(super) host: AsyncHost,
    imports: v8::Global<v8::Object>,
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
        }
    }
}

pub(super) fn callback_context<'s>(args: &v8::FunctionCallbackArguments<'s>) -> &'s AsyncContext {
    let data = args.data();
    assert!(data.is_external());
    let data: v8::Local<v8::Data> = data.into();
    let ptr = v8::Local::<v8::External>::try_from(data).unwrap().value();
    unsafe { &*(ptr as *const AsyncContext) }
}

pub(super) struct ImportArgs<'a, 'scope, 'args> {
    scope: &'a mut v8::HandleScope<'scope>,
    args: &'a v8::FunctionCallbackArguments<'args>,
}

impl<'a, 'scope, 'args> ImportArgs<'a, 'scope, 'args> {
    pub(super) fn new(
        scope: &'a mut v8::HandleScope<'scope>,
        args: &'a v8::FunctionCallbackArguments<'args>,
    ) -> Self {
        Self { scope, args }
    }

    pub(super) fn i32(&mut self, index: i32) -> AsyncHostResult<i32> {
        self.args
            .get(index)
            .int32_value(self.scope)
            .ok_or(AsyncHostError::Inval)
    }

    pub(super) fn i64(&mut self, index: i32) -> AsyncHostResult<i64> {
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
}

fn memory_object<'s>(
    scope: &mut v8::HandleScope<'s>,
    context: &AsyncContext,
) -> AsyncHostResult<v8::Local<'s, v8::WasmMemoryObject>> {
    let imports = v8::Local::new(scope, &context.imports);
    let key = v8::String::new(scope, "memory").ok_or(AsyncHostError::Fault)?;
    let memory = imports
        .get(scope, key.into())
        .ok_or(AsyncHostError::Fault)?;
    v8::Local::<v8::WasmMemoryObject>::try_from(memory).map_err(|_| AsyncHostError::Fault)
}

pub(super) fn with_memory_mut<T>(
    scope: &mut v8::HandleScope,
    context: &AsyncContext,
    f: impl FnOnce(&mut [u8]) -> AsyncHostResult<T>,
) -> AsyncHostResult<T> {
    let memory_object = memory_object(scope, context)?;
    let buffer = memory_object.buffer();
    let len = buffer.byte_length();

    let Some(ptr) = buffer.data() else {
        if len == 0 {
            let mut empty = [];
            return f(&mut empty);
        }
        return Err(AsyncHostError::Fault);
    };

    let memory = unsafe { std::slice::from_raw_parts_mut(ptr.as_ptr() as *mut u8, len) };
    f(memory)
}

pub(super) fn finish_errno(
    context: &AsyncContext,
    ret: &mut v8::ReturnValue,
    result: AsyncHostResult<()>,
) {
    let errno = match result {
        Ok(()) => ASYNC_ERRNO_SUCCESS,
        Err(error) => context.host.record_error(error),
    };
    ret.set_int32(errno);
}

pub(super) fn finish_bool(ret: &mut v8::ReturnValue, value: bool) {
    ret.set_int32(if value { 1 } else { 0 });
}

pub(super) fn throw_import_error(
    scope: &mut v8::HandleScope,
    import_name: &str,
    error: AsyncHostError,
) {
    let message = format!("moonbit_v0.{import_name} failed: {error:?}");
    let message = v8::String::new(scope, &message).unwrap_or_else(|| v8::String::empty(scope));
    let exception = v8::Exception::error(scope, message);
    scope.throw_exception(exception);
}
