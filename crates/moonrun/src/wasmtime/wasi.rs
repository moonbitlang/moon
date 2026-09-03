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

//! Wasmtime adapter for the shared WASIp1 Host state.

use crate::wasi::{self, WasiContext, WasiResult};

use super::StoreData;

pub(super) const WASI_SNAPSHOT_PREVIEW1_MODULE: &str = "wasi_snapshot_preview1";

fn with_memory(
    caller: &mut wasmtime::Caller<'_, StoreData>,
    f: impl FnOnce(&WasiContext, &mut [u8]) -> WasiResult<()>,
) -> i32 {
    let memory = caller
        .get_export("memory")
        .and_then(wasmtime::Extern::into_memory);
    let result = match memory {
        Some(memory) => {
            let (memory, data) = memory.data_and_store_mut(&mut *caller);
            f(data.wasi(), memory)
        }
        None => {
            let mut empty = [];
            f(caller.data().wasi(), &mut empty)
        }
    };
    wasi::result_to_errno(result)
}

pub(super) fn register_imports(linker: &mut wasmtime::Linker<StoreData>) -> wasmtime::Result<()> {
    linker.func_wrap(
        WASI_SNAPSHOT_PREVIEW1_MODULE,
        "args_get",
        |mut caller: wasmtime::Caller<'_, StoreData>, argv_ptr: i32, argv_buf_ptr: i32| {
            with_memory(&mut caller, |context, memory| {
                wasi::args_get_impl(context, memory, argv_ptr as u32, argv_buf_ptr as u32)
            })
        },
    )?;
    linker.func_wrap(
        WASI_SNAPSHOT_PREVIEW1_MODULE,
        "args_sizes_get",
        |mut caller: wasmtime::Caller<'_, StoreData>, argc_ptr: i32, argv_buf_size_ptr: i32| {
            with_memory(&mut caller, |context, memory| {
                wasi::args_sizes_get_impl(
                    context,
                    memory,
                    argc_ptr as u32,
                    argv_buf_size_ptr as u32,
                )
            })
        },
    )?;
    linker.func_wrap(
        WASI_SNAPSHOT_PREVIEW1_MODULE,
        "environ_get",
        |mut caller: wasmtime::Caller<'_, StoreData>, environ_ptr: i32, environ_buf_ptr: i32| {
            with_memory(&mut caller, |context, memory| {
                wasi::environ_get_impl(context, memory, environ_ptr as u32, environ_buf_ptr as u32)
            })
        },
    )?;
    linker.func_wrap(
        WASI_SNAPSHOT_PREVIEW1_MODULE,
        "environ_sizes_get",
        |mut caller: wasmtime::Caller<'_, StoreData>,
         environc_ptr: i32,
         environ_buf_size_ptr: i32| {
            with_memory(&mut caller, |context, memory| {
                wasi::environ_sizes_get_impl(
                    context,
                    memory,
                    environc_ptr as u32,
                    environ_buf_size_ptr as u32,
                )
            })
        },
    )?;
    linker.func_wrap(
        WASI_SNAPSHOT_PREVIEW1_MODULE,
        "random_get",
        |mut caller: wasmtime::Caller<'_, StoreData>, buffer: i32, length: i32| {
            with_memory(&mut caller, |_context, memory| {
                wasi::random_get_impl(memory, buffer as u32, length as u32)
            })
        },
    )?;
    linker.func_wrap(
        WASI_SNAPSHOT_PREVIEW1_MODULE,
        "fd_read",
        |mut caller: wasmtime::Caller<'_, StoreData>,
         fd: i32,
         iovs_ptr: i32,
         iovs_len: i32,
         nread_ptr: i32| {
            with_memory(&mut caller, |context, memory| {
                wasi::fd_read_impl(
                    context,
                    memory,
                    fd,
                    iovs_ptr as u32,
                    iovs_len as u32,
                    nread_ptr as u32,
                )
            })
        },
    )?;
    linker.func_wrap(
        WASI_SNAPSHOT_PREVIEW1_MODULE,
        "fd_write",
        |mut caller: wasmtime::Caller<'_, StoreData>,
         fd: i32,
         iovs_ptr: i32,
         iovs_len: i32,
         nwritten_ptr: i32| {
            with_memory(&mut caller, |context, memory| {
                wasi::fd_write_impl(
                    context,
                    memory,
                    fd,
                    iovs_ptr as u32,
                    iovs_len as u32,
                    nwritten_ptr as u32,
                )
            })
        },
    )?;
    linker.func_wrap(
        WASI_SNAPSHOT_PREVIEW1_MODULE,
        "fd_close",
        |caller: wasmtime::Caller<'_, StoreData>, fd: i32| {
            wasi::result_to_errno(wasi::fd_close_impl(caller.data().wasi(), fd))
        },
    )?;
    linker.func_wrap(
        WASI_SNAPSHOT_PREVIEW1_MODULE,
        "fd_prestat_get",
        |mut caller: wasmtime::Caller<'_, StoreData>, fd: i32, prestat_ptr: i32| {
            with_memory(&mut caller, |context, memory| {
                wasi::fd_prestat_get_impl(context, memory, fd, prestat_ptr as u32)
            })
        },
    )?;
    linker.func_wrap(
        WASI_SNAPSHOT_PREVIEW1_MODULE,
        "fd_prestat_dir_name",
        |mut caller: wasmtime::Caller<'_, StoreData>, fd: i32, path_ptr: i32, path_len: i32| {
            with_memory(&mut caller, |context, memory| {
                wasi::fd_prestat_dir_name_impl(
                    context,
                    memory,
                    fd,
                    path_ptr as u32,
                    path_len as u32,
                )
            })
        },
    )?;
    linker.func_wrap(
        WASI_SNAPSHOT_PREVIEW1_MODULE,
        "fd_readdir",
        |mut caller: wasmtime::Caller<'_, StoreData>,
         fd: i32,
         buf_ptr: i32,
         buf_len: i32,
         cookie: i64,
         buf_used_ptr: i32| {
            with_memory(&mut caller, |context, memory| {
                wasi::fd_readdir_impl(
                    context,
                    memory,
                    fd,
                    buf_ptr as u32,
                    buf_len as u32,
                    cookie as u64,
                    buf_used_ptr as u32,
                )
            })
        },
    )?;
    linker.func_wrap(
        WASI_SNAPSHOT_PREVIEW1_MODULE,
        "path_open",
        |mut caller: wasmtime::Caller<'_, StoreData>,
         dirfd: i32,
         dirflags: i32,
         path_ptr: i32,
         path_len: i32,
         oflags: i32,
         rights_base: i64,
         rights_inheriting: i64,
         fdflags: i32,
         opened_fd_ptr: i32| {
            with_memory(&mut caller, |context, memory| {
                wasi::path_open_impl(
                    context,
                    memory,
                    dirfd,
                    dirflags,
                    path_ptr as u32,
                    path_len as u32,
                    oflags,
                    rights_base as u64,
                    rights_inheriting as u64,
                    fdflags,
                    opened_fd_ptr as u32,
                )
            })
        },
    )?;
    linker.func_wrap(
        WASI_SNAPSHOT_PREVIEW1_MODULE,
        "path_readlink",
        |mut caller: wasmtime::Caller<'_, StoreData>,
         dirfd: i32,
         path_ptr: i32,
         path_len: i32,
         buf_ptr: i32,
         buf_len: i32,
         buf_used_ptr: i32| {
            with_memory(&mut caller, |context, memory| {
                wasi::path_readlink_impl(
                    context,
                    memory,
                    dirfd,
                    path_ptr as u32,
                    path_len as u32,
                    buf_ptr as u32,
                    buf_len as u32,
                    buf_used_ptr as u32,
                )
            })
        },
    )?;
    linker.func_wrap(
        WASI_SNAPSHOT_PREVIEW1_MODULE,
        "path_rename",
        |mut caller: wasmtime::Caller<'_, StoreData>,
         old_fd: i32,
         old_path_ptr: i32,
         old_path_len: i32,
         new_fd: i32,
         new_path_ptr: i32,
         new_path_len: i32| {
            with_memory(&mut caller, |context, memory| {
                wasi::path_rename_impl(
                    context,
                    memory,
                    old_fd,
                    old_path_ptr as u32,
                    old_path_len as u32,
                    new_fd,
                    new_path_ptr as u32,
                    new_path_len as u32,
                )
            })
        },
    )?;
    linker.func_wrap(
        WASI_SNAPSHOT_PREVIEW1_MODULE,
        "path_create_directory",
        |mut caller: wasmtime::Caller<'_, StoreData>, dirfd: i32, path_ptr: i32, path_len: i32| {
            with_memory(&mut caller, |context, memory| {
                wasi::path_create_directory_impl(
                    context,
                    memory,
                    dirfd,
                    path_ptr as u32,
                    path_len as u32,
                )
            })
        },
    )?;
    linker.func_wrap(
        WASI_SNAPSHOT_PREVIEW1_MODULE,
        "path_remove_directory",
        |mut caller: wasmtime::Caller<'_, StoreData>, dirfd: i32, path_ptr: i32, path_len: i32| {
            with_memory(&mut caller, |context, memory| {
                wasi::path_remove_directory_impl(
                    context,
                    memory,
                    dirfd,
                    path_ptr as u32,
                    path_len as u32,
                )
            })
        },
    )?;
    linker.func_wrap(
        WASI_SNAPSHOT_PREVIEW1_MODULE,
        "path_unlink_file",
        |mut caller: wasmtime::Caller<'_, StoreData>, dirfd: i32, path_ptr: i32, path_len: i32| {
            with_memory(&mut caller, |context, memory| {
                wasi::path_unlink_file_impl(
                    context,
                    memory,
                    dirfd,
                    path_ptr as u32,
                    path_len as u32,
                )
            })
        },
    )?;
    linker.func_wrap(
        WASI_SNAPSHOT_PREVIEW1_MODULE,
        "proc_exit",
        |caller: wasmtime::Caller<'_, StoreData>, code: i32| -> wasmtime::Result<()> {
            wasi::proc_exit_impl(caller.data().wasi(), code as u32);
            Err(wasmtime::format_err!("run termination requested"))
        },
    )?;
    Ok(())
}
