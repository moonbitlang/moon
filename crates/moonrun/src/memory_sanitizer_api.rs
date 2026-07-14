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

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use crate::v8_builder::ObjectExt;

pub(crate) const MEMORY_SANITIZER_MODULE: &str = "moonbit:ffi/memory-sanitizer";
const MEMORY_SANITIZER_ENV: &str = "MOONBIT_MEMORY_SANITIZER";

#[derive(Debug)]
struct ObjectRecord {
    size: u32,
    alloc_stack: Rc<SanitizerStack>,
}

#[derive(Debug, Default)]
struct MemorySanitizerState {
    live: BTreeMap<u32, ObjectRecord>,
    // Allocations usually come from a small number of call stacks. Share their
    // frame data so each live object only needs to retain an Rc.
    alloc_stacks: HashSet<Rc<SanitizerStack>>,
}

impl MemorySanitizerState {
    fn register_object_alloc(
        &mut self,
        size: u32,
        ptr: u32,
        capture_alloc_stack: impl FnOnce() -> SanitizerStack,
    ) -> Result<(), MemorySanitizerError> {
        if let Some(record) = self.live.get(&ptr) {
            return Err(MemorySanitizerError::DuplicateObject {
                ptr,
                size: record.size,
                alloc_stack: Rc::clone(&record.alloc_stack),
            });
        }

        let alloc_stack = capture_alloc_stack();
        let alloc_stack = if let Some(interned) = self.alloc_stacks.get(&alloc_stack) {
            Rc::clone(interned)
        } else {
            let alloc_stack = Rc::new(alloc_stack);
            self.alloc_stacks.insert(Rc::clone(&alloc_stack));
            alloc_stack
        };
        self.live.insert(ptr, ObjectRecord { size, alloc_stack });
        Ok(())
    }

    fn register_object_free(&mut self, ptr: u32) -> Result<(), MemorySanitizerError> {
        let record = self
            .live
            .remove(&ptr)
            .ok_or(MemorySanitizerError::InvalidObject { ptr })?;
        if Rc::strong_count(&record.alloc_stack) == 2 {
            self.alloc_stacks.remove(record.alloc_stack.as_ref());
        }
        Ok(())
    }

    fn object_is_valid(&self, ptr: u32) -> bool {
        self.live.contains_key(&ptr)
    }
}

#[derive(Default)]
struct MemorySanitizerContext {
    // V8 invokes these callbacks on the isolate thread; RefCell only provides
    // the interior mutability needed through callback data.
    state: RefCell<MemorySanitizerState>,
}

pub(crate) struct MemorySanitizer {
    context: Box<MemorySanitizerContext>,
}

impl MemorySanitizer {
    pub(crate) fn check_for_leaks_if_enabled(&self) -> Result<(), MemoryLeakError> {
        if std::env::var_os(MEMORY_SANITIZER_ENV).is_none() {
            return Ok(());
        }

        let state = self.context.state.borrow();
        if state.live.is_empty() {
            return Ok(());
        }

        Err(MemoryLeakError {
            objects: state
                .live
                .iter()
                .map(|(&ptr, record)| LeakedObject {
                    ptr,
                    size: record.size,
                    alloc_stack: record.alloc_stack.as_ref().clone(),
                })
                .collect(),
        })
    }
}

#[derive(Debug)]
struct LeakedObject {
    ptr: u32,
    size: u32,
    alloc_stack: SanitizerStack,
}

#[derive(Debug)]
pub(crate) struct MemoryLeakError {
    objects: Vec<LeakedObject>,
}

impl std::fmt::Display for MemoryLeakError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let total_size: u64 = self
            .objects
            .iter()
            .map(|object| u64::from(object.size))
            .sum();
        write!(
            f,
            "moonrun memory sanitizer detected {} leaked object{} ({total_size} bytes)",
            self.objects.len(),
            if self.objects.len() == 1 { "" } else { "s" }
        )?;
        for object in &self.objects {
            write!(f, "\nleaked object {} ({} bytes)", object.ptr, object.size)?;
            object.alloc_stack.write_to(f, "allocation stack")?;
        }
        Ok(())
    }
}

impl std::error::Error for MemoryLeakError {}

#[derive(Debug)]
enum MemorySanitizerError {
    BadArgument,
    DuplicateObject {
        ptr: u32,
        size: u32,
        alloc_stack: Rc<SanitizerStack>,
    },
    InvalidObject {
        ptr: u32,
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
            Self::InvalidObject { ptr } => write!(f, "invalid object {ptr}"),
        }
    }
}

pub(crate) fn init_env<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
) -> MemorySanitizer {
    let context = Box::<MemorySanitizerContext>::default();
    let context_ptr = &*context as *const MemorySanitizerContext;

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

    MemorySanitizer { context }
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
        context
            .state
            .borrow_mut()
            .register_object_alloc(size, ptr, || SanitizerStack::capture(scope))
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
        .and_then(|ptr| context.state.borrow_mut().register_object_free(ptr));
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
        read_u32_arg(scope, &args, 0).map(|ptr| context.state.borrow().object_is_valid(ptr));
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

#[derive(Debug, Clone, Default, Eq, Hash, PartialEq)]
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
            if frame.is_wasm {
                write!(
                    f,
                    "\n    at {}",
                    moonutil::demangle::demangle_mangled_function_name(&frame.raw_function)
                )?;
            } else {
                write!(f, "\n    at {}", frame.raw_function)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
struct SanitizerStackFrame {
    raw_function: String,
    is_wasm: bool,
}

impl SanitizerStackFrame {
    fn from_v8(scope: &mut v8::HandleScope, frame: v8::Local<v8::StackFrame>) -> Self {
        let raw_function = frame
            .get_function_name(scope)
            .map(|name| name.to_rust_string_lossy(scope))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "<anonymous>".to_string());
        Self {
            raw_function,
            is_wasm: frame.is_wasm(),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(function: &str) -> SanitizerStack {
        SanitizerStack {
            frames: vec![SanitizerStackFrame {
                raw_function: function.to_string(),
                is_wasm: true,
            }],
        }
    }

    #[test]
    fn duplicate_allocation_does_not_capture_an_unused_stack() {
        let mut state = MemorySanitizerState::default();
        state
            .register_object_alloc(16, 1024, || stack("first"))
            .unwrap();

        let error = state.register_object_alloc(32, 1024, || {
            panic!("duplicate allocation should reuse the previous stack")
        });

        assert!(matches!(
            error,
            Err(MemorySanitizerError::DuplicateObject {
                ptr: 1024,
                size: 16,
                ..
            })
        ));
    }

    #[test]
    fn allocation_stacks_are_shared_while_live() {
        let mut state = MemorySanitizerState::default();
        state
            .register_object_alloc(16, 1024, || stack("shared"))
            .unwrap();
        state
            .register_object_alloc(16, 2048, || stack("shared"))
            .unwrap();

        assert_eq!(state.alloc_stacks.len(), 1);
        assert!(Rc::ptr_eq(
            &state.live[&1024].alloc_stack,
            &state.live[&2048].alloc_stack
        ));

        state.register_object_free(1024).unwrap();
        assert_eq!(state.alloc_stacks.len(), 1);
        state.register_object_free(2048).unwrap();
        assert!(state.alloc_stacks.is_empty());
    }
}
