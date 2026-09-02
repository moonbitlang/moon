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

//! Host imports installed by Moonrun's current V8 backend.

use super::builder::{ArgsExt, ObjectExt, ScopeExt};
use super::context;
use crate::runtime::Stdio;
use crate::{async_api, filesystem, run_termination, sqlite, util};
use anyhow::Context;
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::any::Any;
use std::cell::Cell;
use std::io;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

const JS_GLUE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/template/js_glue.js"
));

struct PrintEnv {
    dangling_high_half: Cell<Option<u32>>,
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

fn now(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = v8::Array::new(scope, 1);

    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards");

    let secs = v8::Number::new(scope, duration.as_millis() as f64).into();
    result.set_index(scope, 0, secs).unwrap();

    ret.set(result.into());
}

fn instant_now(
    scope: &mut v8::HandleScope,
    mut args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let now = Box::new(Instant::now());
    let ptr = Box::<Instant>::leak(now) as *mut Instant;
    let weak_rc = std::rc::Rc::new(std::cell::Cell::new(None));
    let weak = v8::Weak::with_finalizer(
        unsafe { args.get_isolate() },
        v8::External::new(scope, ptr as *mut std::ffi::c_void),
        Box::new({
            let weak_rc = weak_rc.clone();
            move |isolate| unsafe {
                drop(Box::from_raw(ptr));
                drop(v8::Weak::from_raw(isolate, weak_rc.get()));
            }
        }),
    );
    let local = weak.to_local(scope).unwrap();
    weak_rc.set(weak.into_raw());
    ret.set(local.into());
}

fn instant_elapsed_as_secs_f64(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let arg = args.get(0);
    let instant: v8::Local<v8::External> = arg.try_into().unwrap();
    let instant = unsafe { &*(instant.value() as *mut Instant) };
    let elapsed = instant.elapsed().as_secs_f64();
    ret.set(v8::Number::new(scope, elapsed).into());
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

    let arg = args.get(0);
    let c = arg.integer_value(scope).unwrap() as u32;
    if (0xd800..=0xdbff).contains(&c) {
        let high = c - 0xd800;
        if print_env.dangling_high_half.get().is_some() {
            print_env
                .stdio
                .with_stdout(|stdout| write!(stdout, "\u{fffd}"))
                .unwrap();
        }
        print_env.dangling_high_half.set(Some(high));
    } else {
        let c = if (0xdc00..=0xdfff).contains(&c) {
            print_env
                .dangling_high_half
                .take()
                .map_or(0xfffd, |high| 0x10000 + (high << 10) + (c - 0xdc00))
        } else {
            c
        };
        let c = char::from_u32(c).unwrap();
        print_env
            .stdio
            .with_stdout(|stdout| write!(stdout, "{c}"))
            .unwrap();
    }
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

fn read_char(
    _scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = run_context(&args).runtime().stdio().with_stdin(|stdin| {
        let mut buffer = [0; 4];
        if stdin.read(&mut buffer[..1])? == 0 {
            return Ok(None);
        }

        let num_bytes = match buffer[0] {
            0..=0x7f => 1,
            0xc0..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf7 => 4,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid UTF-8 first byte",
                ));
            }
        };
        if num_bytes > 1 {
            stdin.read_exact(&mut buffer[1..num_bytes])?;
        }

        std::str::from_utf8(&buffer[..num_bytes])
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
            .chars()
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no character found"))
            .map(Some)
    });
    match result {
        Ok(Some(c)) => {
            ret.set_int32(c as i32);
        }
        _ => ret.set_int32(-1),
    }
}

fn read_bytes_from_stdin(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let mut buffer = Vec::new();
    let size = run_context(&args)
        .runtime()
        .stdio()
        .with_stdin(|stdin| stdin.read_to_end(&mut buffer))
        .unwrap();

    if size == 0 {
        let empty_array_buffer = v8::ArrayBuffer::new(scope, 0);
        let empty_uint8_array = v8::Uint8Array::new(scope, empty_array_buffer, 0, 0).unwrap();
        ret.set(empty_uint8_array.into());
    } else {
        let array_buffer = v8::ArrayBuffer::new(scope, size);
        let uint8_array = v8::Uint8Array::new(scope, array_buffer, 0, size).unwrap();
        // SAFETY: V8 owns a writable allocation of `size` bytes for this
        // ArrayBuffer, and `buffer` contains exactly `size` initialized bytes.
        unsafe {
            let array_buffer_ptr: *mut u8 = std::mem::transmute(array_buffer.data());
            std::ptr::copy(buffer.as_ptr(), array_buffer_ptr, size);
        }
        ret.set(uint8_array.into());
    }
}

fn write_char(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _ret: v8::ReturnValue,
) {
    let fd = args.get(0).int32_value(scope).unwrap();
    let c = args.get(1).integer_value(scope).unwrap() as u32;
    let c = std::char::from_u32(c).unwrap();
    let stdio = run_context(&args).runtime().stdio();
    match fd {
        1 => stdio.with_stdout(|stdout| write!(stdout, "{c}")).unwrap(),
        2 => stdio.with_stderr(|stderr| write!(stderr, "{c}")).unwrap(),
        _ => {}
    }
}

fn flush(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _ret: v8::ReturnValue,
) {
    let fd = args.get(0).int32_value(scope).unwrap();
    let stdio = run_context(&args).runtime().stdio();
    match fd {
        1 => stdio.with_stdout(|stdout| stdout.flush()).unwrap(),
        2 => stdio.with_stderr(|stderr| stderr.flush()).unwrap(),
        _ => {}
    }
}

fn stdrng_seed_from_u64(
    scope: &mut v8::HandleScope,
    mut args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let seed = args.get(0).int32_value(scope).unwrap_or(0) as u64;
    let rng = Box::new(StdRng::seed_from_u64(seed));
    let ptr = Box::<StdRng>::leak(rng) as *mut StdRng;
    let weak_rc = std::rc::Rc::new(std::cell::Cell::new(None));
    let weak = v8::Weak::with_finalizer(
        unsafe { args.get_isolate() },
        v8::External::new(scope, ptr as *mut std::ffi::c_void),
        Box::new({
            let weak_rc = weak_rc.clone();
            move |isolate| unsafe {
                drop(Box::from_raw(ptr));
                drop(v8::Weak::from_raw(isolate, weak_rc.get()));
            }
        }),
    );
    let local = weak.to_local(scope).unwrap();
    weak_rc.set(weak.into_raw());
    ret.set(local.into());
}

fn stdrng_gen_range(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let arg = args.get(0);
    let rng: v8::Local<v8::External> = arg.try_into().unwrap();
    let rng = unsafe { &mut *(rng.value() as *mut StdRng) };

    let ubound = args.get(1).int32_value(scope).unwrap();
    let num = rng.gen_range(0..ubound);
    ret.set_int32(num);
}

fn exit(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _ret: v8::ReturnValue,
) {
    let code = args.get(0).to_int32(scope).unwrap();
    let termination_request =
        unsafe { util::get_ref::<run_termination::TerminationRequest>(&args) };
    termination_request.request(run_termination::RunTermination::Exit(code.value()));
    scope.terminate_execution();
}

fn is_windows(
    _scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = if std::env::consts::OS == "windows" {
        1
    } else {
        0
    };
    ret.set_int32(result)
}

/// Build the complete import object and retain the V8 adapter state required
/// after instantiation. Callers do not need to know which imports are Rust
/// callbacks and which require native JavaScript values.
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
    let environment = Arc::clone(runtime.environment());
    let filesystem = Arc::clone(runtime.filesystem());
    let v8_context = Box::new(context::V8RunContext::new(
        runtime,
        termination_request.clone(),
    ));
    let v8_context_ptr = &*v8_context as *const context::V8RunContext;
    let memory_binding = Rc::clone(v8_context.memory_binding());

    let print_env_box = Box::new(PrintEnv {
        dangling_high_half: Cell::new(None),
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
    context::register_func(spectest, scope, "read_char", read_char, v8_context_ptr);
    let moonbit = module_imports.child(scope, "moonbit");
    moonbit.set_value(scope, "string_to_js_string", value.into());
    retained_callback_state.push(print_env_box);

    let console = module_imports.child(scope, "console");
    context::register_func(console, scope, "log", console_log, v8_context_ptr);

    {
        let time = module_imports.child(scope, "__moonbit_time_unstable");
        time.set_func(scope, "instant_now", instant_now);
        time.set_func(
            scope,
            "instant_elapsed_as_secs_f64",
            instant_elapsed_as_secs_f64,
        );
        time.set_func(scope, "now", now);
    }

    {
        let async_runtime = module_imports.child(scope, async_api::MOONBIT_ASYNC_MODULE);
        // SAFETY: the installed imports retain `v8_context` throughout guest
        // execution.
        unsafe { async_api::init_env(async_runtime, scope, v8_context_ptr) };
    }

    {
        let sqlite = module_imports.child(scope, sqlite::v8::MOONBIT_SQLITE_MODULE);
        // SAFETY: the same lifetime invariant as the async adapter above.
        unsafe { sqlite::v8::init_env(sqlite, scope, v8_context_ptr) };
    }

    {
        let wasi = module_imports.child(scope, "wasi_snapshot_preview1");
        super::wasi::init_env(
            wasi,
            scope,
            wasm_file_name,
            args,
            &v8_context,
            &mut retained_callback_state,
        );
    }

    // All V8 callbacks are unreachable after the single-shot run returns, so
    // retaining this one box in the installed imports covers every pointer
    // registered above.
    retained_callback_state.push(v8_context);

    // API for the fs module
    {
        let obj = module_imports.child(scope, "__moonbit_fs_unstable");
        filesystem::v8::init_env(
            obj,
            scope,
            wasm_file_name,
            args,
            environment,
            filesystem,
            &mut retained_callback_state,
        );
    }
    {
        let io = module_imports.child(scope, "__moonbit_io_unstable");
        context::register_func(
            io,
            scope,
            "read_bytes_from_stdin",
            read_bytes_from_stdin,
            v8_context_ptr,
        );
        context::register_func(io, scope, "read_char", read_char, v8_context_ptr);
        context::register_func(io, scope, "write_char", write_char, v8_context_ptr);
        context::register_func(io, scope, "flush", flush, v8_context_ptr);
    }

    {
        let rand = module_imports.child(scope, "__moonbit_rand_unstable");
        rand.set_func(scope, "stdrng_seed_from_u64", stdrng_seed_from_u64);
        rand.set_func(scope, "stdrng_gen_range", stdrng_gen_range);
    }

    {
        let sys = module_imports.child(scope, "__moonbit_sys_unstable");
        let exit_request = Box::new(termination_request.clone());
        let exit_request_ptr = &*exit_request as *const run_termination::TerminationRequest;
        let exit_request_data = v8::External::new(scope, exit_request_ptr as *mut std::ffi::c_void);
        let exit_function = v8::Function::builder(exit)
            .data(exit_request_data.into())
            .build(scope)
            .unwrap();
        sys.set_value(scope, "exit", exit_function.into());
        sys.set_func(scope, "is_windows", is_windows);
        retained_callback_state.push(exit_request);
    }

    let memory_sanitizer_imports =
        module_imports.child(scope, crate::memory_sanitizer::MEMORY_SANITIZER_MODULE);
    super::memory_sanitizer::init_env(
        memory_sanitizer_imports,
        scope,
        memory_sanitizer,
        &mut retained_callback_state,
    );

    // These imports must be JavaScript values rather than Rust callbacks:
    // strings and arrays cross the Wasm boundary as native JS values, while
    // exception and ffi-bytes depend directly on WebAssembly's JS interface.
    let code = scope.string(JS_GLUE);
    let origin_name = format!("{}wasm_mode_entry", super::BUILTIN_SCRIPT_ORIGIN_PREFIX);
    let origin_name = scope.string(&origin_name);
    let script_origin = v8::ScriptOrigin::new(
        scope,
        origin_name.into(),
        0,
        0,
        false,
        0,
        None,
        false,
        false,
        false,
        None,
    );
    let mut source = v8::script_compiler::Source::new(code, Some(&script_origin));
    let module_imports_parameter = scope.string("module_imports");
    let entry = v8::script_compiler::compile_function(
        scope,
        &mut source,
        &[module_imports_parameter],
        &[],
        v8::script_compiler::CompileOptions::NoCompileOptions,
        v8::script_compiler::NoCacheReason::BecauseCachingDisabled,
    )
    .context("failed to compile Moonrun's V8 imports")?;
    {
        let scope = &mut v8::TryCatch::new(scope);
        let receiver = v8::undefined(scope).into();
        if entry
            .call(scope, receiver, &[module_imports.into()])
            .is_none()
        {
            let error = scope
                .stack_trace()
                .or_else(|| scope.exception())
                .context("Moonrun's V8 import setup failed without an exception")?
                .to_rust_string_lossy(scope);
            anyhow::bail!("failed to initialize Moonrun's V8 imports: {error}");
        }
    }

    Ok(InstalledImports {
        module_imports,
        termination_request,
        memory_binding,
        _retained_callback_state: retained_callback_state,
    })
}
