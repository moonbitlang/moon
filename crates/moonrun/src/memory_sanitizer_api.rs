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

//! Host-side `moonbit:ffi/memory-sanitizer` imports.

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::v8_builder::ObjectExt;

pub(crate) const MEMORY_SANITIZER_MODULE: &str = "moonbit:ffi/memory-sanitizer";

#[derive(Debug)]
struct ObjectRecord {
    size: u32,
    alloc_stack: SanitizerStack,
}

#[derive(Debug, Default)]
struct MemorySanitizerState {
    live: BTreeMap<u32, ObjectRecord>,
}

impl MemorySanitizerState {
    fn register_object_alloc(
        &mut self,
        size: u32,
        ptr: u32,
        alloc_stack: SanitizerStack,
    ) -> Result<(), MemorySanitizerError> {
        if let Some(record) = self.live.get(&ptr) {
            return Err(MemorySanitizerError::DuplicateObject {
                ptr,
                size: record.size,
                alloc_stack: record.alloc_stack.clone(),
            });
        }
        self.live.insert(ptr, ObjectRecord { size, alloc_stack });
        Ok(())
    }

    fn register_object_free(
        &mut self,
        ptr: u32,
        free_stack: SanitizerStack,
    ) -> Result<(), MemorySanitizerError> {
        self.live
            .remove(&ptr)
            .map(|_| ())
            .ok_or(MemorySanitizerError::InvalidObject { ptr, free_stack })
    }

    fn object_is_valid(&self, ptr: u32) -> bool {
        self.live.contains_key(&ptr)
    }
}

#[derive(Default)]
struct MemorySanitizerContext {
    state: Mutex<MemorySanitizerState>,
}

#[derive(Debug)]
enum MemorySanitizerError {
    BadArgument,
    DuplicateObject {
        ptr: u32,
        size: u32,
        alloc_stack: SanitizerStack,
    },
    InvalidObject {
        ptr: u32,
        free_stack: SanitizerStack,
    },
}

impl std::fmt::Display for MemorySanitizerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadArgument => write!(f, "invalid argument"),
            Self::DuplicateObject {
                ptr,
                size,
                alloc_stack,
            } => {
                write!(f, "object {ptr} is already live with size {size}")?;
                alloc_stack.write_to(f, "previous allocation stack")
            }
            Self::InvalidObject { ptr, free_stack } => {
                write!(f, "invalid object {ptr}")?;
                free_stack.write_to(f, "free stack")
            }
        }
    }
}

pub(crate) fn init_env<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    dtors: &mut Vec<Box<dyn Any>>,
) {
    let context = Box::<MemorySanitizerContext>::default();
    let context_ptr = &*context as *const MemorySanitizerContext;
    dtors.push(context);

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
}

fn register_func<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
    context_ptr: *const MemorySanitizerContext,
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
    let result = (|| {
        let size = read_u32_arg(scope, &args, 0)?;
        let ptr = read_u32_arg(scope, &args, 1)?;
        let alloc_stack = SanitizerStack::capture(scope);
        context
            .state
            .lock()
            .unwrap()
            .register_object_alloc(size, ptr, alloc_stack)
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
    let result = read_u32_arg(scope, &args, 0).and_then(|ptr| {
        let free_stack = SanitizerStack::capture(scope);
        context
            .state
            .lock()
            .unwrap()
            .register_object_free(ptr, free_stack)
    });
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
    let result =
        read_u32_arg(scope, &args, 0).map(|ptr| context.state.lock().unwrap().object_is_valid(ptr));
    match result {
        Ok(is_valid) => ret.set_bool(is_valid),
        Err(error) => throw_import_error(scope, "object-is-valid", error),
    }
}

fn callback_context<'s>(args: &v8::FunctionCallbackArguments<'s>) -> &'s MemorySanitizerContext {
    let data = args.data();
    assert!(data.is_external());
    let data: v8::Local<v8::Data> = data.into();
    let ptr = v8::Local::<v8::External>::try_from(data).unwrap().value();
    unsafe { &*(ptr as *const MemorySanitizerContext) }
}

#[derive(Debug, Clone, Default)]
struct SanitizerStack {
    frames: Vec<SanitizerStackFrame>,
}

impl SanitizerStack {
    fn capture(scope: &mut v8::HandleScope) -> Self {
        let Some(stack) = v8::StackTrace::current_stack_trace(scope, 32) else {
            return Self::default();
        };
        let mut frames = Vec::with_capacity(stack.get_frame_count());
        for index in 0..stack.get_frame_count() {
            if let Some(frame) = stack.get_frame(scope, index) {
                frames.push(SanitizerStackFrame::from_v8(scope, frame));
            }
        }
        Self { frames }
    }

    fn write_to(&self, f: &mut std::fmt::Formatter<'_>, title: &str) -> std::fmt::Result {
        if self.frames.is_empty() {
            return Ok(());
        }
        write!(f, "\n{title}:")?;
        for frame in &self.frames {
            write!(f, "\n    at {}", frame.function)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct SanitizerStackFrame {
    function: String,
}

impl SanitizerStackFrame {
    fn from_v8(scope: &mut v8::HandleScope, frame: v8::Local<v8::StackFrame>) -> Self {
        let function = frame
            .get_function_name(scope)
            .map(|name| name.to_rust_string_lossy(scope))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "<anonymous>".to_string());
        let function = if frame.is_wasm() {
            moonutil::demangle::demangle_mangled_function_name(&function)
        } else {
            function
        };
        Self { function }
    }
}

fn read_u32_arg(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    index: i32,
) -> Result<u32, MemorySanitizerError> {
    args.get(index)
        .uint32_value(scope)
        .ok_or(MemorySanitizerError::BadArgument)
}

fn throw_import_error(scope: &mut v8::HandleScope, import_name: &str, error: MemorySanitizerError) {
    let message = format!("{MEMORY_SANITIZER_MODULE}.{import_name} failed: {error}");
    let message = v8::String::new(scope, &message).unwrap_or_else(|| v8::String::empty(scope));
    let exception = v8::Exception::error(scope, message);
    scope.throw_exception(exception);
}
