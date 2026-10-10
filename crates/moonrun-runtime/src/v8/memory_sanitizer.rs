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

//! V8 adapter for the shared memory-sanitizer Host state.

use std::any::Any;

use super::builder::ObjectExt;
use crate::memory_sanitizer::{
    MEMORY_SANITIZER_MODULE, MemorySanitizer, MemorySanitizerError, SanitizerStack,
    SanitizerStackFrame,
};

struct V8MemorySanitizerContext {
    host: MemorySanitizer,
}

impl std::ops::Deref for V8MemorySanitizerContext {
    type Target = MemorySanitizer;

    fn deref(&self) -> &Self::Target {
        &self.host
    }
}

#[derive(Debug)]
enum AdapterError {
    BadArgument,
    Host(MemorySanitizerError),
}

impl From<MemorySanitizerError> for AdapterError {
    fn from(error: MemorySanitizerError) -> Self {
        Self::Host(error)
    }
}

impl std::fmt::Display for AdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadArgument => write!(f, "invalid argument"),
            Self::Host(error) => error.fmt(f),
        }
    }
}

pub(super) fn init_env<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    memory_sanitizer: &MemorySanitizer,
    dtors: &mut Vec<Box<dyn Any>>,
) {
    let context = Box::new(V8MemorySanitizerContext {
        host: memory_sanitizer.clone(),
    });
    let context_ptr = &*context as *const V8MemorySanitizerContext;
    register_func(
        obj,
        scope,
        "register-object-alloc",
        register_object_alloc,
        context_ptr,
    );
    register_func(
        obj,
        scope,
        "register-object-free",
        register_object_free,
        context_ptr,
    );
    register_func(obj, scope, "object-is-valid", object_is_valid, context_ptr);
    dtors.push(context);
}

fn register_func<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
    context_ptr: *const V8MemorySanitizerContext,
) {
    let data = v8::External::new(scope, context_ptr as *mut std::ffi::c_void);
    let function = v8::Function::builder(callback)
        .data(data.into())
        .build(scope)
        .unwrap();
    obj.set_value(scope, name, function.into());
}

fn register_object_alloc(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| -> Result<(), AdapterError> {
        let size = read_u32_arg(scope, &args, 0)?;
        let ptr = read_u32_arg(scope, &args, 1)?;
        context
            .register_object_alloc(size, ptr, || capture_stack(scope))
            .map_err(Into::into)
    })();
    match result {
        Ok(()) => ret.set_undefined(),
        Err(error) => throw_import_error(scope, "register-object-alloc", error),
    }
}

fn register_object_free(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = read_u32_arg(scope, &args, 0)
        .and_then(|ptr| context.register_object_free(ptr).map_err(Into::into));
    match result {
        Ok(()) => ret.set_undefined(),
        Err(error) => throw_import_error(scope, "register-object-free", error),
    }
}

fn object_is_valid(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = read_u32_arg(scope, &args, 0).map(|ptr| context.object_is_valid(ptr));
    match result {
        Ok(is_valid) => ret.set_bool(is_valid),
        Err(error) => throw_import_error(scope, "object-is-valid", error),
    }
}

fn callback_context<'s>(args: &v8::FunctionCallbackArguments<'s>) -> &'s V8MemorySanitizerContext {
    let data = args.data();
    assert!(data.is_external());
    let data: v8::Local<v8::Data> = data.into();
    let ptr = v8::Local::<v8::External>::try_from(data).unwrap().value();
    unsafe { &*(ptr as *const V8MemorySanitizerContext) }
}

fn capture_stack(scope: &mut v8::HandleScope) -> SanitizerStack {
    let Some(stack) = v8::StackTrace::current_stack_trace(scope, 32) else {
        return SanitizerStack::default();
    };
    let mut frames = Vec::with_capacity(stack.get_frame_count());
    for index in 0..stack.get_frame_count() {
        if let Some(frame) = stack.get_frame(scope, index) {
            frames.push(stack_frame(scope, frame));
        }
    }
    SanitizerStack::new(frames)
}

fn stack_frame(
    scope: &mut v8::HandleScope,
    frame: v8::Local<v8::StackFrame>,
) -> SanitizerStackFrame {
    let raw_function = frame
        .get_function_name(scope)
        .map(|name| name.to_rust_string_lossy(scope))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "<anonymous>".to_string());
    SanitizerStackFrame::new(raw_function, frame.is_wasm())
}

fn read_u32_arg(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    index: i32,
) -> Result<u32, AdapterError> {
    args.get(index)
        .uint32_value(scope)
        .ok_or(AdapterError::BadArgument)
}

fn throw_import_error(scope: &mut v8::HandleScope, import_name: &str, error: AdapterError) {
    let message = format!("{MEMORY_SANITIZER_MODULE}.{import_name} failed: {error}");
    let message = v8::String::new(scope, &message).unwrap_or_else(|| v8::String::empty(scope));
    let exception = v8::Exception::error(scope, message);
    scope.throw_exception(exception);
}
