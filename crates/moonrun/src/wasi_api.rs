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

use crate::v8_builder::ScopeExt;
use rand::RngCore;
use std::any::Any;
use std::io::{Read, Write};
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

type WasiErrno = i32;
type WasiResult<T> = Result<T, WasiErrno>;

const WASI_ERRNO_SUCCESS: WasiErrno = 0;
const WASI_ERRNO_BADF: WasiErrno = 8;
const WASI_ERRNO_FAULT: WasiErrno = 21;
const WASI_ERRNO_INVAL: WasiErrno = 28;
const WASI_ERRNO_IO: WasiErrno = 29;

const WASI_FD_STDIN: i32 = 0;
const WASI_FD_STDOUT: i32 = 1;
const WASI_FD_STDERR: i32 = 2;
const WASI_IOVEC_SIZE: usize = 8;

#[repr(i32)]
#[derive(Clone, Copy)]
enum ClockId {
    Realtime = 0,
    Monotonic = 1,
    ProcessCpuTime = 2,
    ThreadCpuTime = 3,
}

impl TryFrom<i32> for ClockId {
    type Error = WasiErrno;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Realtime),
            1 => Ok(Self::Monotonic),
            2 => Ok(Self::ProcessCpuTime),
            3 => Ok(Self::ThreadCpuTime),
            _ => Err(WASI_ERRNO_INVAL),
        }
    }
}

struct WasiContext {
    argv: Vec<Vec<u8>>,
    monotonic_origin: Instant,
    memory: OnceLock<v8::Global<v8::WasmMemoryObject>>,
}

fn encode_c_string(value: impl Into<String>) -> Vec<u8> {
    let mut bytes = value.into().into_bytes();
    bytes.push(0);
    bytes
}

fn build_argv(wasm_file_name: &str, args: &[String]) -> Vec<Vec<u8>> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(encode_c_string(wasm_file_name));
    argv.extend(args.iter().map(|arg| encode_c_string(arg.as_str())));
    argv
}

fn collect_environ() -> Vec<Vec<u8>> {
    std::env::vars()
        .map(|(key, value)| encode_c_string(format!("{key}={value}")))
        .collect()
}

fn read_i32_arg(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    index: i32,
) -> WasiResult<i32> {
    args.get(index).int32_value(scope).ok_or(WASI_ERRNO_INVAL)
}

fn ptr_to_offset(ptr: i32) -> WasiResult<usize> {
    usize::try_from(ptr).map_err(|_| WASI_ERRNO_FAULT)
}

fn checked_mut_range(memory: &mut [u8], offset: usize, len: usize) -> WasiResult<&mut [u8]> {
    let end = offset.checked_add(len).ok_or(WASI_ERRNO_FAULT)?;
    memory.get_mut(offset..end).ok_or(WASI_ERRNO_FAULT)
}

fn write_u32_at(memory: &mut [u8], offset: usize, value: u32) -> WasiResult<()> {
    checked_mut_range(memory, offset, 4)?.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u32(memory: &mut [u8], ptr: i32, value: u32) -> WasiResult<()> {
    write_u32_at(memory, ptr_to_offset(ptr)?, value)
}

fn read_u32_at(memory: &[u8], offset: usize) -> WasiResult<u32> {
    let end = offset.checked_add(4).ok_or(WASI_ERRNO_FAULT)?;
    let bytes = memory.get(offset..end).ok_or(WASI_ERRNO_FAULT)?;
    Ok(u32::from_le_bytes(
        <[u8; 4]>::try_from(bytes).map_err(|_| WASI_ERRNO_FAULT)?,
    ))
}

fn write_u64(memory: &mut [u8], ptr: i32, value: u64) -> WasiResult<()> {
    let offset = ptr_to_offset(ptr)?;
    checked_mut_range(memory, offset, 8)?.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn table_bytes_len(values: &[Vec<u8>]) -> WasiResult<u32> {
    let total = values.iter().try_fold(0usize, |acc, value| {
        acc.checked_add(value.len()).ok_or(WASI_ERRNO_FAULT)
    })?;
    u32::try_from(total).map_err(|_| WASI_ERRNO_FAULT)
}

fn write_c_string_table(
    memory: &mut [u8],
    values: &[Vec<u8>],
    pointers_ptr: i32,
    bytes_ptr: i32,
) -> WasiResult<()> {
    let pointers_base = ptr_to_offset(pointers_ptr)?;
    let mut cursor = ptr_to_offset(bytes_ptr)?;

    for (index, value) in values.iter().enumerate() {
        let pointer_slot = pointers_base
            .checked_add(index.checked_mul(4).ok_or(WASI_ERRNO_FAULT)?)
            .ok_or(WASI_ERRNO_FAULT)?;

        let cursor_u32 = u32::try_from(cursor).map_err(|_| WASI_ERRNO_FAULT)?;
        write_u32_at(memory, pointer_slot, cursor_u32)?;

        checked_mut_range(memory, cursor, value.len())?.copy_from_slice(value);
        cursor = cursor.checked_add(value.len()).ok_or(WASI_ERRNO_FAULT)?;
    }

    Ok(())
}

fn iovec(memory: &[u8], iovs_ptr: i32, index: u32) -> WasiResult<(usize, usize)> {
    let base = ptr_to_offset(iovs_ptr)?;
    let index_offset = usize::try_from(index).map_err(|_| WASI_ERRNO_FAULT)?;
    let iov_offset = base
        .checked_add(
            index_offset
                .checked_mul(WASI_IOVEC_SIZE)
                .ok_or(WASI_ERRNO_FAULT)?,
        )
        .ok_or(WASI_ERRNO_FAULT)?;

    let buf_ptr = read_u32_at(memory, iov_offset)?;
    let buf_len = read_u32_at(memory, iov_offset + 4)?;
    let buf_offset = usize::try_from(buf_ptr).map_err(|_| WASI_ERRNO_FAULT)?;
    let len = usize::try_from(buf_len).map_err(|_| WASI_ERRNO_FAULT)?;
    Ok((buf_offset, len))
}

fn callback_context<'s>(args: &v8::FunctionCallbackArguments<'s>) -> &'s WasiContext {
    let data = args.data();
    assert!(data.is_external());
    let data: v8::Local<v8::Data> = data.into();
    let ptr = v8::Local::<v8::External>::try_from(data).unwrap().value();
    unsafe { &*(ptr as *const WasiContext) }
}

fn cached_wasi_memory<'s>(
    scope: &mut v8::HandleScope<'s>,
    context: &WasiContext,
) -> WasiResult<v8::Local<'s, v8::WasmMemoryObject>> {
    context
        .memory
        .get()
        .map(|memory| v8::Local::new(scope, memory))
        .ok_or(WASI_ERRNO_FAULT)
}

fn with_wasi_memory_mut(
    scope: &mut v8::HandleScope,
    context: &WasiContext,
    f: impl FnOnce(&mut [u8]) -> WasiResult<()>,
) -> WasiResult<()> {
    let memory_object = cached_wasi_memory(scope, context)?;
    let buffer = memory_object.buffer();
    let len = buffer.byte_length();

    let Some(ptr) = buffer.data() else {
        if len == 0 {
            let mut empty = [];
            return f(&mut empty);
        }
        return Err(WASI_ERRNO_FAULT);
    };

    let memory = unsafe { std::slice::from_raw_parts_mut(ptr.as_ptr() as *mut u8, len) };
    f(memory)
}

fn result_to_errno(result: WasiResult<()>) -> WasiErrno {
    match result {
        Ok(()) => WASI_ERRNO_SUCCESS,
        Err(errno) => errno,
    }
}

fn finish_with_result(ret: &mut v8::ReturnValue, result: WasiResult<()>) {
    ret.set_int32(result_to_errno(result));
}

fn set_memory(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let context = callback_context(&args);
        let memory_value = args.get(0);
        let memory = v8::Local::<v8::WasmMemoryObject>::try_from(memory_value)
            .map_err(|_| WASI_ERRNO_INVAL)?;
        let _ = context.memory.set(v8::Global::new(scope, memory));
        Ok(())
    })();
    finish_with_result(&mut ret, result);
}

fn args_sizes_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let argc_ptr = read_i32_arg(scope, &args, 0)?;
        let argv_buf_size_ptr = read_i32_arg(scope, &args, 1)?;

        let context = callback_context(&args);
        let argc = u32::try_from(context.argv.len()).map_err(|_| WASI_ERRNO_FAULT)?;
        let argv_buf_size = table_bytes_len(&context.argv)?;

        with_wasi_memory_mut(scope, context, |memory| {
            write_u32(memory, argc_ptr, argc)?;
            write_u32(memory, argv_buf_size_ptr, argv_buf_size)?;
            Ok(())
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
        let argv_ptr = read_i32_arg(scope, &args, 0)?;
        let argv_buf_ptr = read_i32_arg(scope, &args, 1)?;
        let context = callback_context(&args);

        with_wasi_memory_mut(scope, context, |memory| {
            write_c_string_table(memory, &context.argv, argv_ptr, argv_buf_ptr)
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
        let environc_ptr = read_i32_arg(scope, &args, 0)?;
        let environ_buf_size_ptr = read_i32_arg(scope, &args, 1)?;

        let environ = collect_environ();
        let environc = u32::try_from(environ.len()).map_err(|_| WASI_ERRNO_FAULT)?;
        let environ_buf_size = table_bytes_len(&environ)?;
        let context = callback_context(&args);

        with_wasi_memory_mut(scope, context, |memory| {
            write_u32(memory, environc_ptr, environc)?;
            write_u32(memory, environ_buf_size_ptr, environ_buf_size)?;
            Ok(())
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
        let environ_ptr = read_i32_arg(scope, &args, 0)?;
        let environ_buf_ptr = read_i32_arg(scope, &args, 1)?;
        let context = callback_context(&args);

        let environ = collect_environ();
        with_wasi_memory_mut(scope, context, |memory| {
            write_c_string_table(memory, &environ, environ_ptr, environ_buf_ptr)
        })
    })();

    finish_with_result(&mut ret, result);
}

fn random_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let buf_ptr = read_i32_arg(scope, &args, 0)?;
        let buf_len =
            usize::try_from(read_i32_arg(scope, &args, 1)?).map_err(|_| WASI_ERRNO_INVAL)?;
        let context = callback_context(&args);

        with_wasi_memory_mut(scope, context, |memory| {
            let buf = checked_mut_range(memory, ptr_to_offset(buf_ptr)?, buf_len)?;
            rand::thread_rng().fill_bytes(buf);
            Ok(())
        })
    })();

    finish_with_result(&mut ret, result);
}

fn fd_write(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let fd = read_i32_arg(scope, &args, 0)?;
        let iovs_ptr = read_i32_arg(scope, &args, 1)?;
        let iovs_len =
            u32::try_from(read_i32_arg(scope, &args, 2)?).map_err(|_| WASI_ERRNO_INVAL)?;
        let nwritten_ptr = read_i32_arg(scope, &args, 3)?;
        let context = callback_context(&args);

        with_wasi_memory_mut(scope, context, |memory| {
            let mut total_written: usize = 0;

            match fd {
                WASI_FD_STDOUT => {
                    let mut stdout = std::io::stdout();
                    for index in 0..iovs_len {
                        let (buf_offset, len) = iovec(memory, iovs_ptr, index)?;
                        let bytes = checked_mut_range(memory, buf_offset, len)?;
                        stdout.write_all(bytes).map_err(|_| WASI_ERRNO_IO)?;
                        total_written = total_written.checked_add(len).ok_or(WASI_ERRNO_FAULT)?;
                    }
                    stdout.flush().map_err(|_| WASI_ERRNO_IO)?;
                }
                WASI_FD_STDERR => {
                    let mut stderr = std::io::stderr();
                    for index in 0..iovs_len {
                        let (buf_offset, len) = iovec(memory, iovs_ptr, index)?;
                        let bytes = checked_mut_range(memory, buf_offset, len)?;
                        stderr.write_all(bytes).map_err(|_| WASI_ERRNO_IO)?;
                        total_written = total_written.checked_add(len).ok_or(WASI_ERRNO_FAULT)?;
                    }
                    stderr.flush().map_err(|_| WASI_ERRNO_IO)?;
                }
                _ => return Err(WASI_ERRNO_BADF),
            }

            let nwritten = u32::try_from(total_written).map_err(|_| WASI_ERRNO_FAULT)?;
            write_u32(memory, nwritten_ptr, nwritten)?;
            Ok(())
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
        let iovs_ptr = read_i32_arg(scope, &args, 1)?;
        let iovs_len =
            u32::try_from(read_i32_arg(scope, &args, 2)?).map_err(|_| WASI_ERRNO_INVAL)?;
        let nread_ptr = read_i32_arg(scope, &args, 3)?;
        if fd != WASI_FD_STDIN {
            return Err(WASI_ERRNO_BADF);
        }

        let context = callback_context(&args);

        with_wasi_memory_mut(scope, context, |memory| {
            let mut stdin = std::io::stdin().lock();
            let mut total_read: usize = 0;

            for index in 0..iovs_len {
                let (buf_offset, len) = iovec(memory, iovs_ptr, index)?;
                if len == 0 {
                    continue;
                }
                let buffer = checked_mut_range(memory, buf_offset, len)?;
                let read_len = stdin.read(buffer).map_err(|_| WASI_ERRNO_IO)?;
                total_read = total_read.checked_add(read_len).ok_or(WASI_ERRNO_FAULT)?;

                if read_len < len {
                    break;
                }
            }

            let nread = u32::try_from(total_read).map_err(|_| WASI_ERRNO_FAULT)?;
            write_u32(memory, nread_ptr, nread)?;
            Ok(())
        })
    })();

    finish_with_result(&mut ret, result);
}

fn clock_now_ns(context: &WasiContext, clock_id: ClockId) -> WasiResult<u64> {
    match clock_id {
        ClockId::Realtime => {
            let duration = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| WASI_ERRNO_FAULT)?;
            Ok(duration.as_nanos().min(u128::from(u64::MAX)) as u64)
        }
        ClockId::Monotonic | ClockId::ProcessCpuTime | ClockId::ThreadCpuTime => {
            let elapsed = context.monotonic_origin.elapsed();
            Ok(elapsed.as_nanos().min(u128::from(u64::MAX)) as u64)
        }
    }
}

fn clock_resolution_ns(clock_id: ClockId) -> WasiResult<u64> {
    match clock_id {
        ClockId::Realtime
        | ClockId::Monotonic
        | ClockId::ProcessCpuTime
        | ClockId::ThreadCpuTime => Ok(1),
    }
}

fn clock_res_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let clock_id = ClockId::try_from(read_i32_arg(scope, &args, 0)?)?;
        let resolution_ptr = read_i32_arg(scope, &args, 1)?;
        let resolution = clock_resolution_ns(clock_id)?;
        let context = callback_context(&args);

        with_wasi_memory_mut(scope, context, |memory| {
            write_u64(memory, resolution_ptr, resolution)?;
            Ok(())
        })
    })();

    finish_with_result(&mut ret, result);
}

fn clock_time_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let clock_id = ClockId::try_from(read_i32_arg(scope, &args, 0)?)?;
        let time_ptr = read_i32_arg(scope, &args, 2)?;

        let context = callback_context(&args);
        let now_ns = clock_now_ns(context, clock_id)?;

        with_wasi_memory_mut(scope, context, |memory| {
            write_u64(memory, time_ptr, now_ns)?;
            Ok(())
        })
    })();

    finish_with_result(&mut ret, result);
}

fn set_wasi_func<'s>(
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

pub(crate) fn init_env<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    wasm_file_name: &str,
    args: &[String],
    dtors: &mut Vec<Box<dyn Any>>,
) {
    let context = Box::new(WasiContext {
        argv: build_argv(wasm_file_name, args),
        monotonic_origin: Instant::now(),
        memory: OnceLock::new(),
    });
    let context_ptr = &*context as *const WasiContext as *mut std::ffi::c_void;

    set_wasi_func(obj, scope, "set_memory", set_memory, context_ptr);
    set_wasi_func(obj, scope, "args_get", args_get, context_ptr);
    set_wasi_func(obj, scope, "args_sizes_get", args_sizes_get, context_ptr);
    set_wasi_func(obj, scope, "environ_get", environ_get, context_ptr);
    set_wasi_func(
        obj,
        scope,
        "environ_sizes_get",
        environ_sizes_get,
        context_ptr,
    );
    set_wasi_func(obj, scope, "fd_read", fd_read, context_ptr);
    set_wasi_func(obj, scope, "fd_write", fd_write, context_ptr);
    set_wasi_func(obj, scope, "random_get", random_get, context_ptr);
    set_wasi_func(obj, scope, "clock_res_get", clock_res_get, context_ptr);
    set_wasi_func(obj, scope, "clock_time_get", clock_time_get, context_ptr);

    dtors.push(context);
}
