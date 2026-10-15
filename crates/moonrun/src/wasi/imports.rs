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

//! The shared WASIp1 subset supported by all three engine adapters.

use super::{WASI_ERRNO_FAULT, WasiContext, WasiResult, result_to_errno};
use crate::wasm_imports::{Caller, ImportError, ImportRegistry};

pub(crate) const MODULE: &str = "wasi_snapshot_preview1";

pub(crate) fn register<R: ImportRegistry<WasiContext>>(registry: &mut R) -> Result<(), R::Error> {
    registry.func_wrap(
        MODULE,
        "args_sizes_get",
        |caller: Caller<'_, WasiContext>, argc: i32, size: i32| {
            with_memory(caller, |data, memory| {
                super::args_sizes_get_impl(data, memory, argc as u32, size as u32)
            })
        },
    )?;
    registry.func_wrap(
        MODULE,
        "args_get",
        |caller: Caller<'_, WasiContext>, argv: i32, bytes: i32| {
            with_memory(caller, |data, memory| {
                super::args_get_impl(data, memory, argv as u32, bytes as u32)
            })
        },
    )?;
    registry.func_wrap(
        MODULE,
        "fd_write",
        |caller: Caller<'_, WasiContext>, fd: i32, iovs: i32, count: i32, written: i32| {
            with_memory(caller, |data, memory| {
                super::fd_write_impl(data, memory, fd, iovs as u32, count as u32, written as u32)
            })
        },
    )?;
    registry.func_wrap(
        MODULE,
        "environ_sizes_get",
        |caller: Caller<'_, WasiContext>, count: i32, size: i32| {
            with_memory(caller, |data, memory| {
                super::environ_sizes_get_impl(data, memory, count as u32, size as u32)
            })
        },
    )?;
    registry.func_wrap(
        MODULE,
        "environ_get",
        |caller: Caller<'_, WasiContext>, environ: i32, bytes: i32| {
            with_memory(caller, |data, memory| {
                super::environ_get_impl(data, memory, environ as u32, bytes as u32)
            })
        },
    )?;
    registry.func_wrap(
        MODULE,
        "random_get",
        |caller: Caller<'_, WasiContext>, buffer: i32, length: i32| {
            with_memory(caller, |_data, memory| {
                super::random_get_impl(memory, buffer as u32, length as u32)
            })
        },
    )?;
    registry.func_wrap(
        MODULE,
        "fd_read",
        |caller: Caller<'_, WasiContext>, fd: i32, iovs: i32, count: i32, read: i32| {
            with_memory(caller, |data, memory| {
                super::fd_read_impl(data, memory, fd, iovs as u32, count as u32, read as u32)
            })
        },
    )?;
    registry.func_wrap(
        MODULE,
        "fd_prestat_get",
        |caller: Caller<'_, WasiContext>, fd: i32, prestat: i32| {
            with_memory(caller, |data, memory| {
                super::fd_prestat_get_impl(data, memory, fd, prestat as u32)
            })
        },
    )?;
    registry.func_wrap(
        MODULE,
        "fd_prestat_dir_name",
        |caller: Caller<'_, WasiContext>, fd: i32, path: i32, length: i32| {
            with_memory(caller, |data, memory| {
                super::fd_prestat_dir_name_impl(data, memory, fd, path as u32, length as u32)
            })
        },
    )?;
    registry.func_wrap(
        MODULE,
        "fd_readdir",
        |caller: Caller<'_, WasiContext>,
         fd: i32,
         buffer: i32,
         length: i32,
         cookie: i64,
         used: i32| {
            with_memory(caller, |data, memory| {
                super::fd_readdir_impl(
                    data,
                    memory,
                    fd,
                    buffer as u32,
                    length as u32,
                    cookie as u64,
                    used as u32,
                )
            })
        },
    )?;
    registry.func_wrap(
        MODULE,
        "path_open",
        |caller: Caller<'_, WasiContext>,
         dirfd: i32,
         dirflags: i32,
         path: i32,
         length: i32,
         oflags: i32,
         rights: i64,
         inheriting: i64,
         fdflags: i32,
         opened: i32| {
            with_memory(caller, |data, memory| {
                super::path_open_impl(
                    data,
                    memory,
                    dirfd,
                    dirflags,
                    path as u32,
                    length as u32,
                    oflags,
                    rights as u64,
                    inheriting as u64,
                    fdflags,
                    opened as u32,
                )
            })
        },
    )?;
    registry.func_wrap(
        MODULE,
        "path_readlink",
        |caller: Caller<'_, WasiContext>,
         dirfd: i32,
         path: i32,
         length: i32,
         buffer: i32,
         capacity: i32,
         used: i32| {
            with_memory(caller, |data, memory| {
                super::path_readlink_impl(
                    data,
                    memory,
                    dirfd,
                    path as u32,
                    length as u32,
                    buffer as u32,
                    capacity as u32,
                    used as u32,
                )
            })
        },
    )?;
    registry.func_wrap(
        MODULE,
        "path_rename",
        |caller: Caller<'_, WasiContext>,
         old_fd: i32,
         old_path: i32,
         old_length: i32,
         new_fd: i32,
         new_path: i32,
         new_length: i32| {
            with_memory(caller, |data, memory| {
                super::path_rename_impl(
                    data,
                    memory,
                    old_fd,
                    old_path as u32,
                    old_length as u32,
                    new_fd,
                    new_path as u32,
                    new_length as u32,
                )
            })
        },
    )?;
    registry.func_wrap(
        MODULE,
        "path_create_directory",
        |caller: Caller<'_, WasiContext>, dirfd: i32, path: i32, length: i32| {
            with_memory(caller, |data, memory| {
                super::path_create_directory_impl(data, memory, dirfd, path as u32, length as u32)
            })
        },
    )?;
    registry.func_wrap(
        MODULE,
        "path_remove_directory",
        |caller: Caller<'_, WasiContext>, dirfd: i32, path: i32, length: i32| {
            with_memory(caller, |data, memory| {
                super::path_remove_directory_impl(data, memory, dirfd, path as u32, length as u32)
            })
        },
    )?;
    registry.func_wrap(
        MODULE,
        "path_unlink_file",
        |caller: Caller<'_, WasiContext>, dirfd: i32, path: i32, length: i32| {
            with_memory(caller, |data, memory| {
                super::path_unlink_file_impl(data, memory, dirfd, path as u32, length as u32)
            })
        },
    )?;
    registry.func_wrap(
        MODULE,
        "fd_close",
        |caller: Caller<'_, WasiContext>, fd: i32| {
            Ok(result_to_errno(super::fd_close_impl(caller.data, fd)))
        },
    )?;
    registry.func_wrap(
        MODULE,
        "proc_exit",
        |caller: Caller<'_, WasiContext>, code: i32| -> Result<(), ImportError> {
            super::proc_exit_impl(caller.data, code as u32);
            Err(ImportError::Terminated)
        },
    )
}

fn with_memory(
    caller: Caller<'_, WasiContext>,
    operation: impl FnOnce(&WasiContext, &mut [u8]) -> WasiResult<()>,
) -> Result<i32, ImportError> {
    // V8 has historically returned EFAULT for an unbound memory. wee8 instead
    // reports a host error for memory access during module start. Preserve that
    // adapter limitation without imposing it on memory-free operations.
    match caller.memory {
        Ok(memory) => Ok(result_to_errno(operation(caller.data, memory))),
        Err(ImportError::MemoryUnavailable) => Ok(WASI_ERRNO_FAULT),
        Err(error) => Err(error),
    }
}
