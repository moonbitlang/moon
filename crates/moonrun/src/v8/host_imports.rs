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

//! Host imports installed by Moonrun's current V8 backend.

use super::builder::{ArgsExt, ObjectExt};
use super::context;
use crate::runtime::{Stdio, Utf16Writer};
use crate::{async_api, core_api, run_termination, sqlite};
use std::any::Any;
use std::rc::Rc;
use std::sync::Arc;

struct PrintEnv {
    writer: Utf16Writer,
    stdio: Arc<Stdio>,
}

pub(super) struct InstalledImports<'s> {
    pub(super) module_imports: v8::Local<'s, v8::Object>,
    pub(super) termination_request: run_termination::TerminationRequest,
    pub(super) memory_binding: Rc<context::V8MemoryBinding>,
    _retained_callback_state: Vec<Box<dyn Any>>,
}

fn run_context<'s>(args: &v8::FunctionCallbackArguments<'s>) -> &'s context::V8RunContext {
    // SAFETY: install registers these callbacks with the retained
    // V8RunContext pointer.
    unsafe { context::callback_context(args) }
}

fn print_char(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let print_env = {
        let data = args.data();
        assert!(data.is_external());
        let data: v8::Local<v8::Data> = data.into();
        let ptr = v8::Local::<v8::External>::try_from(data).unwrap().value();
        unsafe { &*(ptr as *const PrintEnv) }
    };

    print_env
        .writer
        .write_stdout(
            &print_env.stdio,
            args.get(0).integer_value(scope).unwrap() as u32,
        )
        .unwrap();
    ret.set_undefined()
}

fn console_log(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _ret: v8::ReturnValue,
) {
    let arg = args.string_lossy(scope, 0);
    run_context(&args)
        .runtime()
        .stdio()
        .with_stdout(|stdout| writeln!(stdout, "{arg}"))
        .unwrap();
}

/// Build the complete import object and retain the V8 adapter state required
/// after instantiation. Callers do not need to know which imports are Rust
/// callbacks or V8-native values.
pub(super) fn install<'s>(
    scope: &mut v8::HandleScope<'s>,
    wasm_file_name: &str,
    args: &[String],
    runtime: crate::runtime::Runtime,
    memory_sanitizer: &crate::memory_sanitizer::MemorySanitizer,
) -> anyhow::Result<InstalledImports<'s>> {
    let module_imports = v8::Object::new(scope);
    let mut retained_callback_state = Vec::<Box<dyn Any>>::new();
    let termination_request = run_termination::TerminationRequest::default();
    let values = Rc::new(super::values::State::new(scope)?);
    let v8_context = Box::new(context::V8RunContext::new(
        runtime,
        termination_request.clone(),
    ));
    let v8_context_ptr = &*v8_context as *const context::V8RunContext;
    let memory_binding = Rc::clone(v8_context.memory_binding());

    let print_env_box = Box::new(PrintEnv {
        writer: Utf16Writer::default(),
        stdio: Arc::clone(v8_context.runtime().stdio()),
    });
    let print_env = &*print_env_box as *const PrintEnv;
    let print_env = v8::External::new(scope, print_env as *mut std::ffi::c_void);
    let value = v8::Function::builder(print_char)
        .data(print_env.into())
        .build(scope)
        .unwrap();
    let spectest = module_imports.child(scope, "spectest");
    spectest.set_value(scope, "print_char", value.into());
    let moonbit = module_imports.child(scope, "moonbit");
    moonbit.set_value(scope, "string_to_js_string", value.into());
    retained_callback_state.push(print_env_box);

    let console = module_imports.child(scope, "console");
    context::register_func(console, scope, "log", console_log, v8_context_ptr);

    {
        let mut registry = super::wasm_imports::Registry {
            object: module_imports,
            scope,
            data: Rc::clone(v8_context.runtime()),
            memory: Rc::clone(&memory_binding),
            termination: termination_request.clone(),
            values: Rc::clone(&values),
            retained: &mut retained_callback_state,
        };
        async_api::register(&mut registry).unwrap();
        core_api::register(&mut registry).unwrap();
        sqlite::wasm::register(&mut registry).unwrap();
        crate::host_imports::register(&mut registry, wasm_file_name, args).unwrap();
    }

    {
        super::wasi::init_env(
            module_imports,
            scope,
            wasm_file_name,
            args,
            &v8_context,
            values,
            &mut retained_callback_state,
        );
    }

    // All V8 callbacks are unreachable after the single-shot run returns, so
    // retaining this one box in the installed imports covers every pointer
    // registered above.
    retained_callback_state.push(v8_context);

    let memory_sanitizer_imports =
        module_imports.child(scope, crate::memory_sanitizer::MEMORY_SANITIZER_MODULE);
    super::memory_sanitizer::init_env(
        memory_sanitizer_imports,
        scope,
        memory_sanitizer,
        &mut retained_callback_state,
    );

    let bytes = module_imports.child(scope, "ffi-bytes");
    super::ffi_bytes::install(scope, bytes)?;
    let exception = module_imports.child(scope, "exception");
    super::exception::install(scope, exception, &mut retained_callback_state)?;

    Ok(InstalledImports {
        module_imports,
        termination_request,
        memory_binding,
        _retained_callback_state: retained_callback_state,
    })
}
