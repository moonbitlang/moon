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

use crate::v8_builder::{ArgsExt, ObjectExt, ScopeExt};
use crate::{
    async_api, async_policy, backtrace_api, fs_api_temp, run_termination, sys_api, util, wasi_api,
};
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::any::Any;
use std::io::{self, Write};
use std::sync::Arc;
use std::{cell::Cell, io::Read, path::PathBuf, time::Instant};

#[derive(Default)]
struct PrintEnv {
    dangling_high_half: Cell<Option<u32>>,
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
        // high surrogate
        let high = c - 0xd800;
        if print_env.dangling_high_half.get().is_some() {
            // Print previous char as invalid unicode
            print!("{}", std::char::from_u32(0xfffd).unwrap());
        }
        print_env.dangling_high_half.set(Some(high));
    } else {
        let c = {
            if (0xdc00..=0xdfff).contains(&c) {
                // low surrogate
                if let Some(high) = print_env.dangling_high_half.take() {
                    0x10000 + (high << 10) + (c - 0xdc00)
                } else {
                    0xfffd
                }
            } else {
                c
            }
        };
        let c = std::char::from_u32(c).unwrap();
        print!("{c}");
    }
    ret.set_undefined()
}

fn console_elog(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _ret: v8::ReturnValue,
) {
    let arg = args.string_lossy(scope, 0);
    eprintln!("{arg}");
}

fn console_log(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _ret: v8::ReturnValue,
) {
    let arg = args.string_lossy(scope, 0);
    println!("{arg}");
}

fn get_array_buffer_ptr(ab: v8::Local<v8::ArrayBuffer>) -> *mut u8 {
    unsafe { std::mem::transmute(ab.data()) }
}

fn read_utf8_char() -> io::Result<Option<char>> {
    let mut buffer = [0; 4];
    let stdin = io::stdin();
    let mut handle = stdin.lock();

    let size = handle.read(&mut buffer[0..1])?;
    if size == 0 {
        return Ok(None);
    }

    let num_bytes = match buffer[0] {
        0..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid UTF-8 first byte",
            ));
        }
    };

    if num_bytes > 1 {
        handle.read_exact(&mut buffer[1..num_bytes])?;
    }

    let char = std::str::from_utf8(&buffer[..num_bytes])
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
        .chars()
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no character found"))?;

    Ok(Some(char))
}

fn read_char(
    _scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = read_utf8_char();
    match result {
        Ok(Some(c)) => {
            ret.set_int32(c as i32);
        }
        _ => ret.set_int32(-1),
    }
}

fn read_bytes_from_stdin(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let mut buffer = Vec::new();
    let stdin = io::stdin();
    let mut handle = stdin.lock();

    let size = handle.read_to_end(&mut buffer).unwrap();

    if size == 0 {
        let empty_array_buffer = v8::ArrayBuffer::new(scope, 0);
        let empty_uint8_array = v8::Uint8Array::new(scope, empty_array_buffer, 0, 0).unwrap();
        ret.set(empty_uint8_array.into());
    } else {
        let array_buffer = v8::ArrayBuffer::new(scope, size);
        let uint8_array = v8::Uint8Array::new(scope, array_buffer, 0, size).unwrap();
        unsafe {
            std::ptr::copy(buffer.as_ptr(), get_array_buffer_ptr(array_buffer), size);
        }
        ret.set(uint8_array.into());
    }
}

fn read_file_to_bytes(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = PathBuf::from(args.string_lossy(scope, 0));
    let Ok(bytes) = std::fs::read(path) else {
        ret.set_undefined();
        return;
    };
    let buffer = v8::ArrayBuffer::new(scope, bytes.len());
    let Some(ab) = v8::Uint8Array::new(scope, buffer, 0, bytes.len()) else {
        ret.set_undefined();
        return;
    };

    unsafe {
        std::ptr::copy(bytes.as_ptr(), get_array_buffer_ptr(buffer), bytes.len());
    }

    ret.set(ab.into());
}

fn write_char(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _ret: v8::ReturnValue,
) {
    let fd = args.get(0).int32_value(scope).unwrap();
    let c = args.get(1).integer_value(scope).unwrap() as u32;
    let c = std::char::from_u32(c).unwrap();
    match fd {
        1 => print!("{c}"),
        2 => eprint!("{c}"),
        _ => {}
    }
}

fn flush(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _ret: v8::ReturnValue,
) {
    let fd = args.get(0).int32_value(scope).unwrap();
    match fd {
        1 => std::io::stdout().flush().unwrap(),
        2 => std::io::stderr().flush().unwrap(),
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

pub(crate) fn install(
    dtors: &mut Vec<Box<dyn Any>>,
    scope: &mut v8::HandleScope,
    wasm_file_name: &str,
    args: &[String],
    async_policy: Arc<async_policy::AsyncPolicy>,
) -> run_termination::TerminationRequest {
    let global_proxy = scope.get_current_context().global(scope);
    let termination_request = run_termination::TerminationRequest::default();

    let print_env_box = Box::<PrintEnv>::default();
    let identifier = scope.string("print");
    let print_env = &*print_env_box as *const PrintEnv;
    let print_env = v8::External::new(scope, print_env as *mut std::ffi::c_void);
    let value = v8::Function::builder(print_char)
        .data(print_env.into())
        .build(scope)
        .unwrap();
    global_proxy.set(scope, identifier.into(), value.into());
    dtors.push(print_env_box);

    {
        global_proxy.set_func(scope, "console_log", console_log);
        global_proxy.set_func(scope, "console_elog", console_elog);
    }

    {
        let time = global_proxy.child(scope, "__moonbit_time_unstable");
        time.set_func(scope, "instant_now", instant_now);
        time.set_func(
            scope,
            "instant_elapsed_as_secs_f64",
            instant_elapsed_as_secs_f64,
        );
        time.set_func(scope, "now", now);
    }

    {
        let async_runtime = global_proxy.child(scope, async_api::MOONBIT_ASYNC_MODULE);
        async_api::init_env(
            async_runtime,
            scope,
            dtors,
            Arc::clone(&async_policy),
            termination_request.clone(),
        );
    }

    {
        let wasi = global_proxy.child(scope, "__moonbit_wasi_unstable");
        wasi_api::init_env(
            wasi,
            scope,
            wasm_file_name,
            args,
            termination_request.clone(),
            dtors,
        );
    }

    // API for the fs module
    {
        let obj = global_proxy.child(scope, "__moonbit_fs_unstable");
        sys_api::init_env(
            obj,
            scope,
            wasm_file_name,
            args,
            Arc::clone(&async_policy),
            dtors,
        );
        fs_api_temp::init_fs(obj, scope, Arc::clone(&async_policy), dtors);
    }
    backtrace_api::init(scope);

    {
        global_proxy.set_func(scope, "read_file_to_bytes", read_file_to_bytes);
    }

    {
        let io = global_proxy.child(scope, "__moonbit_io_unstable");
        io.set_func(scope, "read_bytes_from_stdin", read_bytes_from_stdin);
        io.set_func(scope, "read_char", read_char);
        io.set_func(scope, "write_char", write_char);
        io.set_func(scope, "flush", flush);
    }

    {
        let rand = global_proxy.child(scope, "__moonbit_rand_unstable");
        rand.set_func(scope, "stdrng_seed_from_u64", stdrng_seed_from_u64);
        rand.set_func(scope, "stdrng_gen_range", stdrng_gen_range);
    }

    {
        let sys = global_proxy.child(scope, "__moonbit_sys_unstable");
        let exit_request = Box::new(termination_request.clone());
        let exit_request_ptr = &*exit_request as *const run_termination::TerminationRequest;
        let exit_request_data = v8::External::new(scope, exit_request_ptr as *mut std::ffi::c_void);
        let exit_function = v8::Function::builder(exit)
            .data(exit_request_data.into())
            .build(scope)
            .unwrap();
        sys.set_value(scope, "exit", exit_function.into());
        sys.set_func(scope, "is_windows", is_windows);
        dtors.push(exit_request);
    }
    termination_request
}
