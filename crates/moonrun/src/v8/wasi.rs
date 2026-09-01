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

//! V8 adapter for the shared WASIp1 Host state.

use std::any::Any;
use std::rc::Rc;

use super::builder::ScopeExt;
use super::context::{self, V8ImportError, V8MemoryBinding, V8RunContext};
use crate::wasi::{self, WASI_ERRNO_FAULT, WASI_ERRNO_INVAL, WasiContext, WasiResult};

struct V8WasiContext {
    host: WasiContext,
    memory_binding: Rc<V8MemoryBinding>,
}

impl std::ops::Deref for V8WasiContext {
    type Target = WasiContext;

    fn deref(&self) -> &Self::Target {
        &self.host
    }
}

fn read_i32_arg(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    index: i32,
) -> WasiResult<i32> {
    args.get(index).int32_value(scope).ok_or(WASI_ERRNO_INVAL)
}

fn read_u32_arg(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    index: i32,
) -> WasiResult<u32> {
    args.get(index).uint32_value(scope).ok_or(WASI_ERRNO_INVAL)
}

fn read_u64_arg(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    index: i32,
) -> WasiResult<u64> {
    let value = args.get(index);
    if value.is_big_int() {
        context::decode_wasm_u64(value).ok_or(WASI_ERRNO_INVAL)
    } else {
        let value = value.integer_value(scope).ok_or(WASI_ERRNO_INVAL)?;
        u64::try_from(value).map_err(|_| WASI_ERRNO_INVAL)
    }
}

fn callback_context<'s>(args: &v8::FunctionCallbackArguments<'s>) -> &'s V8WasiContext {
    let data = args.data();
    assert!(data.is_external());
    let data: v8::Local<v8::Data> = data.into();
    let ptr = v8::Local::<v8::External>::try_from(data).unwrap().value();
    unsafe { &*(ptr as *const V8WasiContext) }
}

fn with_wasi_memory_mut<T>(
    scope: &mut v8::HandleScope,
    context: &V8WasiContext,
    f: impl FnOnce(&mut [u8]) -> WasiResult<T>,
) -> WasiResult<T> {
    context
        .memory_binding
        .with_memory_mut(scope, |memory| {
            Ok::<WasiResult<T>, V8ImportError>(f(memory))
        })
        .map_err(|_| WASI_ERRNO_FAULT)?
}

fn finish_with_result(ret: &mut v8::ReturnValue, result: WasiResult<()>) {
    ret.set_int32(wasi::result_to_errno(result));
}

fn random_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let buffer = read_u32_arg(scope, &args, 0)?;
        let length = read_u32_arg(scope, &args, 1)?;
        let context = callback_context(&args);
        with_wasi_memory_mut(scope, context, |memory| {
            wasi::random_get_impl(memory, buffer, length)
        })
    })();
    finish_with_result(&mut ret, result);
}

fn fd_close(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let fd = read_i32_arg(scope, &args, 0)?;
        let context = callback_context(&args);
        wasi::fd_close_impl(context, fd)
    })();

    finish_with_result(&mut ret, result);
}

fn path_open(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let dirfd = read_i32_arg(scope, &args, 0)?;
        let dirflags = read_i32_arg(scope, &args, 1)?;
        let path_ptr = read_u32_arg(scope, &args, 2)?;
        let path_len = read_u32_arg(scope, &args, 3)?;
        let oflags = read_i32_arg(scope, &args, 4)?;
        let rights_base = read_u64_arg(scope, &args, 5)?;
        let rights_inheriting = read_u64_arg(scope, &args, 6)?;
        let fdflags = read_i32_arg(scope, &args, 7)?;
        let opened_fd_ptr = read_u32_arg(scope, &args, 8)?;
        let context = callback_context(&args);

        with_wasi_memory_mut(scope, context, |memory| {
            wasi::path_open_impl(
                context,
                memory,
                dirfd,
                dirflags,
                path_ptr,
                path_len,
                oflags,
                rights_base,
                rights_inheriting,
                fdflags,
                opened_fd_ptr,
            )
        })
    })();

    finish_with_result(&mut ret, result);
}

fn path_readlink(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let dirfd = read_i32_arg(scope, &args, 0)?;
        let path_ptr = read_u32_arg(scope, &args, 1)?;
        let path_len = read_u32_arg(scope, &args, 2)?;
        let buf_ptr = read_u32_arg(scope, &args, 3)?;
        let buf_len = read_u32_arg(scope, &args, 4)?;
        let buf_used_ptr = read_u32_arg(scope, &args, 5)?;
        let context = callback_context(&args);
        with_wasi_memory_mut(scope, context, |memory| {
            wasi::path_readlink_impl(
                context,
                memory,
                dirfd,
                path_ptr,
                path_len,
                buf_ptr,
                buf_len,
                buf_used_ptr,
            )
        })
    })();

    finish_with_result(&mut ret, result);
}

fn path_create_directory(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let dirfd = read_i32_arg(scope, &args, 0)?;
        let path_ptr = read_u32_arg(scope, &args, 1)?;
        let path_len = read_u32_arg(scope, &args, 2)?;
        let context = callback_context(&args);
        with_wasi_memory_mut(scope, context, |memory| {
            wasi::path_create_directory_impl(context, memory, dirfd, path_ptr, path_len)
        })
    })();

    finish_with_result(&mut ret, result);
}

fn path_rename(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let old_fd = read_i32_arg(scope, &args, 0)?;
        let old_path_ptr = read_u32_arg(scope, &args, 1)?;
        let old_path_len = read_u32_arg(scope, &args, 2)?;
        let new_fd = read_i32_arg(scope, &args, 3)?;
        let new_path_ptr = read_u32_arg(scope, &args, 4)?;
        let new_path_len = read_u32_arg(scope, &args, 5)?;
        let context = callback_context(&args);
        with_wasi_memory_mut(scope, context, |memory| {
            wasi::path_rename_impl(
                context,
                memory,
                old_fd,
                old_path_ptr,
                old_path_len,
                new_fd,
                new_path_ptr,
                new_path_len,
            )
        })
    })();

    finish_with_result(&mut ret, result);
}

fn path_remove_directory(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let dirfd = read_i32_arg(scope, &args, 0)?;
        let path_ptr = read_u32_arg(scope, &args, 1)?;
        let path_len = read_u32_arg(scope, &args, 2)?;
        let context = callback_context(&args);
        with_wasi_memory_mut(scope, context, |memory| {
            wasi::path_remove_directory_impl(context, memory, dirfd, path_ptr, path_len)
        })
    })();

    finish_with_result(&mut ret, result);
}

fn path_unlink_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let dirfd = read_i32_arg(scope, &args, 0)?;
        let path_ptr = read_u32_arg(scope, &args, 1)?;
        let path_len = read_u32_arg(scope, &args, 2)?;
        let context = callback_context(&args);
        with_wasi_memory_mut(scope, context, |memory| {
            wasi::path_unlink_file_impl(context, memory, dirfd, path_ptr, path_len)
        })
    })();

    finish_with_result(&mut ret, result);
}

fn fd_prestat_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let fd = read_i32_arg(scope, &args, 0)?;
        let prestat_ptr = read_u32_arg(scope, &args, 1)?;
        let context = callback_context(&args);
        with_wasi_memory_mut(scope, context, |memory| {
            wasi::fd_prestat_get_impl(context, memory, fd, prestat_ptr)
        })
    })();

    finish_with_result(&mut ret, result);
}

fn fd_prestat_dir_name(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let fd = read_i32_arg(scope, &args, 0)?;
        let path_ptr = read_u32_arg(scope, &args, 1)?;
        let path_len = read_u32_arg(scope, &args, 2)?;
        let context = callback_context(&args);
        with_wasi_memory_mut(scope, context, |memory| {
            wasi::fd_prestat_dir_name_impl(context, memory, fd, path_ptr, path_len)
        })
    })();

    finish_with_result(&mut ret, result);
}

fn fd_readdir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let fd = read_i32_arg(scope, &args, 0)?;
        let buf_ptr = read_u32_arg(scope, &args, 1)?;
        let buf_len = read_u32_arg(scope, &args, 2)?;
        let cookie = read_u64_arg(scope, &args, 3)?;
        let buf_used_ptr = read_u32_arg(scope, &args, 4)?;
        let context = callback_context(&args);
        with_wasi_memory_mut(scope, context, |memory| {
            wasi::fd_readdir_impl(context, memory, fd, buf_ptr, buf_len, cookie, buf_used_ptr)
        })
    })();

    finish_with_result(&mut ret, result);
}

fn args_sizes_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let argc_ptr = read_u32_arg(scope, &args, 0)?;
        let argv_buf_size_ptr = read_u32_arg(scope, &args, 1)?;

        let context = callback_context(&args);
        with_wasi_memory_mut(scope, context, |memory| {
            wasi::args_sizes_get_impl(context, memory, argc_ptr, argv_buf_size_ptr)
        })
    })();

    finish_with_result(&mut ret, result);
}

fn args_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let argv_ptr = read_u32_arg(scope, &args, 0)?;
        let argv_buf_ptr = read_u32_arg(scope, &args, 1)?;
        let context = callback_context(&args);

        with_wasi_memory_mut(scope, context, |memory| {
            wasi::args_get_impl(context, memory, argv_ptr, argv_buf_ptr)
        })
    })();

    finish_with_result(&mut ret, result);
}

fn environ_sizes_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let environc_ptr = read_u32_arg(scope, &args, 0)?;
        let environ_buf_size_ptr = read_u32_arg(scope, &args, 1)?;
        let context = callback_context(&args);

        with_wasi_memory_mut(scope, context, |memory| {
            wasi::environ_sizes_get_impl(context, memory, environc_ptr, environ_buf_size_ptr)
        })
    })();

    finish_with_result(&mut ret, result);
}

fn environ_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let environ_ptr = read_u32_arg(scope, &args, 0)?;
        let environ_buf_ptr = read_u32_arg(scope, &args, 1)?;
        let context = callback_context(&args);

        with_wasi_memory_mut(scope, context, |memory| {
            wasi::environ_get_impl(context, memory, environ_ptr, environ_buf_ptr)
        })
    })();

    finish_with_result(&mut ret, result);
}

fn proc_exit(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _ret: v8::ReturnValue,
) {
    let code = args.get(0).uint32_value(scope).unwrap_or(1);
    wasi::proc_exit_impl(callback_context(&args), code);
    scope.terminate_execution();
}

fn fd_write(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let fd = read_i32_arg(scope, &args, 0)?;
        let iovs_ptr = read_u32_arg(scope, &args, 1)?;
        let iovs_len = read_u32_arg(scope, &args, 2)?;
        let nwritten_ptr = read_u32_arg(scope, &args, 3)?;
        let context = callback_context(&args);
        with_wasi_memory_mut(scope, context, |memory| {
            wasi::fd_write_impl(context, memory, fd, iovs_ptr, iovs_len, nwritten_ptr)
        })
    })();

    finish_with_result(&mut ret, result);
}

fn fd_read(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let fd = read_i32_arg(scope, &args, 0)?;
        let iovs_ptr = read_u32_arg(scope, &args, 1)?;
        let iovs_len = read_u32_arg(scope, &args, 2)?;
        let nread_ptr = read_u32_arg(scope, &args, 3)?;
        let context = callback_context(&args);
        with_wasi_memory_mut(scope, context, |memory| {
            wasi::fd_read_impl(context, memory, fd, iovs_ptr, iovs_len, nread_ptr)
        })
    })();

    finish_with_result(&mut ret, result);
}

fn set_wasi_func_impl<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
    context_ptr: *mut std::ffi::c_void,
) {
    let key = scope.string(name);
    let data = v8::External::new(scope, context_ptr);
    let function = v8::Function::builder(callback)
        .data(data.into())
        .build(scope)
        .unwrap();
    obj.set(scope, key.into(), function.into());
}

macro_rules! set_wasi_func {
    ($obj:expr, $scope:expr, $context_ptr:expr, $callback:ident) => {
        set_wasi_func_impl($obj, $scope, stringify!($callback), $callback, $context_ptr);
    };
}

pub(super) fn init_env<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    wasm_file_name: &str,
    args: &[String],
    run_context: &V8RunContext,
    dtors: &mut Vec<Box<dyn Any>>,
) {
    let context = Box::new(V8WasiContext {
        host: WasiContext::new(
            wasm_file_name,
            args,
            run_context.runtime(),
            run_context.termination_request().clone(),
        ),
        memory_binding: Rc::clone(run_context.memory_binding()),
    });
    let context_ptr = &*context as *const V8WasiContext as *mut std::ffi::c_void;

    set_wasi_func!(obj, scope, context_ptr, args_get);
    set_wasi_func!(obj, scope, context_ptr, args_sizes_get);
    set_wasi_func!(obj, scope, context_ptr, environ_get);
    set_wasi_func!(obj, scope, context_ptr, environ_sizes_get);
    set_wasi_func!(obj, scope, context_ptr, random_get);
    set_wasi_func!(obj, scope, context_ptr, fd_read);
    set_wasi_func!(obj, scope, context_ptr, fd_write);
    set_wasi_func!(obj, scope, context_ptr, fd_close);
    set_wasi_func!(obj, scope, context_ptr, fd_prestat_get);
    set_wasi_func!(obj, scope, context_ptr, fd_prestat_dir_name);
    set_wasi_func!(obj, scope, context_ptr, fd_readdir);
    set_wasi_func!(obj, scope, context_ptr, path_open);
    set_wasi_func!(obj, scope, context_ptr, path_readlink);
    set_wasi_func!(obj, scope, context_ptr, path_rename);
    set_wasi_func!(obj, scope, context_ptr, path_create_directory);
    set_wasi_func!(obj, scope, context_ptr, path_remove_directory);
    set_wasi_func!(obj, scope, context_ptr, path_unlink_file);
    set_wasi_func!(obj, scope, context_ptr, proc_exit);

    dtors.push(context);
}
