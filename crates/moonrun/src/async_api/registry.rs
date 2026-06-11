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

use crate::v8_builder::ObjectExt;

use super::{
    c_buffer, context::AsyncContext, env_util, event_loop, fd_util, fs, memory, os_error, process,
    thread_pool, time, unsupported,
};

pub(crate) const MOONBIT_V0_MODULE: &str = "moonbit_v0";
#[cfg(test)]
const NATIVE_ASYNC_PREFIX: &str = "moonbitlang_async_";

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AsyncImportKind {
    NativeMapped,
    UnsupportedMvp,
    WasmSupport,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceRoot {
    MoonbitAsync,
    Moonrun,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceLocation {
    root: SourceRoot,
    path: &'static str,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AsyncImport {
    kind: AsyncImportKind,
    wasm_symbol: &'static str,
    native_symbol: Option<&'static str>,
    sources: &'static [SourceLocation],
}

#[cfg(test)]
macro_rules! import_kind {
    (native) => {
        AsyncImportKind::NativeMapped
    };
    (unsupported) => {
        AsyncImportKind::UnsupportedMvp
    };
    (support) => {
        AsyncImportKind::WasmSupport
    };
}

#[cfg(test)]
macro_rules! source_root {
    (moonbit_async) => {
        SourceRoot::MoonbitAsync
    };
    (moonrun) => {
        SourceRoot::Moonrun
    };
}

macro_rules! declare_async_imports {
    ($(
        $kind:ident $callback:path => $wasm_symbol:literal,
        native = $native_symbol:expr,
        sources = [$($source_root:ident:$source_path:literal),+ $(,)?];
    )*) => {
        #[cfg(test)]
        const ASYNC_IMPORTS: &[AsyncImport] = &[
            $(
                AsyncImport {
                    kind: import_kind!($kind),
                    wasm_symbol: $wasm_symbol,
                    native_symbol: $native_symbol,
                    sources: &[
                        $(
                            SourceLocation {
                                root: source_root!($source_root),
                                path: $source_path,
                            },
                        )+
                    ],
                },
            )*
        ];

        pub(super) fn register_imports<'s>(
            obj: v8::Local<'s, v8::Object>,
            scope: &mut v8::HandleScope<'s>,
            context_ptr: *const AsyncContext,
        ) {
            $(
                register_func_impl(obj, scope, $wasm_symbol, $callback, context_ptr);
            )*
        }
    };
}

fn register_func_impl<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
    context_ptr: *const AsyncContext,
) {
    let data = v8::External::new(scope, context_ptr as *mut std::ffi::c_void);
    let function = v8::Function::builder(callback)
        .data(data.into())
        .build(scope)
        .unwrap();
    obj.set_value(scope, name, function.into());
}

declare_async_imports! {
    native event_loop::get_platform => "get_platform",
    native = Some("moonbitlang_async_get_platform"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native event_loop::errno_is_cancelled => "errno_is_cancelled",
    native = Some("moonbitlang_async_errno_is_cancelled"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::job_get_ret => "job_get_ret",
    native = Some("moonbitlang_async_job_get_ret"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::job_get_err => "job_get_err",
    native = Some("moonbitlang_async_job_get_err"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    support thread_pool::free_job => "free_job",
    native = None,
    sources = [moonrun:"crates/moonrun/src/async_api/thread_pool.rs"];

    support thread_pool::run_job => "run_job",
    native = None,
    sources = [moonrun:"crates/moonrun/src/async_api/thread_pool.rs"];

    support thread_pool::complete_job => "complete_job",
    native = None,
    sources = [moonrun:"crates/moonrun/src/async_api/thread_pool.rs"];

    native thread_pool::spawn_worker => "spawn_worker",
    native = Some("moonbitlang_async_spawn_worker"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::free_worker => "free_worker",
    native = Some("moonbitlang_async_free_worker"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::wake_worker => "wake_worker",
    native = Some("moonbitlang_async_wake_worker"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::worker_enter_idle => "worker_enter_idle",
    native = Some("moonbitlang_async_worker_enter_idle"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::cancel_worker => "cancel_worker",
    native = Some("moonbitlang_async_cancel_worker"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::fetch_completion => "fetch_completion",
    native = Some("moonbitlang_async_fetch_completion"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_sleep_job => "make_sleep_job",
    native = Some("moonbitlang_async_make_sleep_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    support process::spawn_process => "spawn_process",
    native = None,
    sources = [moonrun:"crates/moonrun/src/async_api/process.rs"];

    native process::make_wait_for_process_job => "make_wait_for_process_job",
    native = Some("moonbitlang_async_make_wait_for_process_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native time::get_ms_since_epoch => "get_ms_since_epoch",
    native = Some("moonbitlang_async_get_ms_since_epoch"),
    sources = [moonbit_async:"src/internal/time/time.c"];

    support time::sleep_ms => "sleep_ms",
    native = None,
    sources = [moonbit_async:"src/internal/event_loop/event_loop.wasm.mbt"];

    support memory::copy_from_guest => "copy_from_guest",
    native = None,
    sources = [moonrun:"crates/moonrun/src/async_host/mod.rs"];

    support memory::zero_guest => "zero_guest",
    native = None,
    sources = [moonrun:"crates/moonrun/src/async_host/mod.rs"];

    native os_error::get_errno => "get_errno",
    native = Some("moonbitlang_async_get_errno"),
    sources = [moonbit_async:"src/os_error/stub.c"];

    native os_error::is_nonblocking_io_error => "is_nonblocking_io_error",
    native = Some("moonbitlang_async_is_nonblocking_io_error"),
    sources = [moonbit_async:"src/os_error/stub.c"];

    native os_error::is_eintr => "is_EINTR",
    native = Some("moonbitlang_async_is_EINTR"),
    sources = [moonbit_async:"src/os_error/stub.c"];

    native os_error::is_enoent => "is_ENOENT",
    native = Some("moonbitlang_async_is_ENOENT"),
    sources = [moonbit_async:"src/os_error/stub.c"];

    native os_error::is_eexist => "is_EEXIST",
    native = Some("moonbitlang_async_is_EEXIST"),
    sources = [moonbit_async:"src/os_error/stub.c"];

    native os_error::is_eacces => "is_EACCES",
    native = Some("moonbitlang_async_is_EACCES"),
    sources = [moonbit_async:"src/os_error/stub.c"];

    native os_error::is_econnrefused => "is_ECONNREFUSED",
    native = Some("moonbitlang_async_is_ECONNREFUSED"),
    sources = [moonbit_async:"src/os_error/stub.c"];

    native os_error::is_error_notify_enum_dir => "is_ERROR_NOTIFY_ENUM_DIR",
    native = Some("moonbitlang_async_is_ERROR_NOTIFY_ENUM_DIR"),
    sources = [moonbit_async:"src/os_error/stub.c"];

    native os_error::get_enotdir => "get_ENOTDIR",
    native = Some("moonbitlang_async_get_ENOTDIR"),
    sources = [moonbit_async:"src/os_error/stub.c"];

    unsupported unsupported::i32 => "get_invalid_handle",
    native = Some("moonbitlang_async_get_invalid_handle"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    native fs::close_fd => "close_fd",
    native = Some("moonbitlang_async_close_fd"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    unsupported unsupported::i32 => "fd_is_nonblocking",
    native = Some("moonbitlang_async_fd_is_nonblocking"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    unsupported unsupported::i32 => "set_blocking",
    native = Some("moonbitlang_async_set_blocking"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    unsupported unsupported::i32 => "set_nonblocking",
    native = Some("moonbitlang_async_set_nonblocking"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    unsupported unsupported::i32 => "set_cloexec",
    native = Some("moonbitlang_async_set_cloexec"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    unsupported unsupported::i32 => "create_named_pipe_server",
    native = Some("moonbitlang_async_create_named_pipe_server"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    unsupported unsupported::i32 => "create_named_pipe_client",
    native = Some("moonbitlang_async_create_named_pipe_client"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    native fd_util::pipe => "pipe",
    native = Some("moonbitlang_async_pipe"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    native fd_util::sizeof_file_time => "sizeof_file_time",
    native = Some("moonbitlang_async_sizeof_file_time"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    native fd_util::get_atime_sec => "get_atime_sec",
    native = Some("moonbitlang_async_get_atime_sec"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    native fd_util::get_atime_nsec => "get_atime_nsec",
    native = Some("moonbitlang_async_get_atime_nsec"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    native fd_util::get_mtime_sec => "get_mtime_sec",
    native = Some("moonbitlang_async_get_mtime_sec"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    native fd_util::get_mtime_nsec => "get_mtime_nsec",
    native = Some("moonbitlang_async_get_mtime_nsec"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    native fd_util::get_ctime_sec => "get_ctime_sec",
    native = Some("moonbitlang_async_get_ctime_sec"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    native fd_util::get_ctime_nsec => "get_ctime_nsec",
    native = Some("moonbitlang_async_get_ctime_nsec"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    native env_util::getpid => "getpid",
    native = Some("moonbitlang_async_getpid"),
    sources = [moonbit_async:"src/internal/env_util/stub.c"];

    native c_buffer::blit_to_c => "blit_to_c",
    native = Some("moonbitlang_async_blit_to_c"),
    sources = [moonbit_async:"src/internal/c_buffer/stub.c"];

    native c_buffer::blit_from_c => "blit_from_c",
    native = Some("moonbitlang_async_blit_from_c"),
    sources = [moonbit_async:"src/internal/c_buffer/stub.c"];

    native c_buffer::c_buffer_get => "c_buffer_get",
    native = Some("moonbitlang_async_c_buffer_get"),
    sources = [moonbit_async:"src/internal/c_buffer/stub.c"];

    native c_buffer::strlen => "strlen",
    native = Some("moonbitlang_async_strlen"),
    sources = [moonbit_async:"src/internal/c_buffer/stub.c"];

    native c_buffer::null_pointer => "null_pointer",
    native = Some("moonbitlang_async_null_pointer"),
    sources = [moonbit_async:"src/internal/c_buffer/stub.c"];

    native c_buffer::pointer_is_null => "pointer_is_null",
    native = Some("moonbitlang_async_pointer_is_null"),
    sources = [moonbit_async:"src/internal/c_buffer/stub.c"];

    unsupported unsupported::i32 => "c_buffer_as_string",
    native = Some("moonbitlang_async_c_buffer_as_string"),
    sources = [moonbit_async:"src/internal/os_string/stub.c"];

    native fs::errno_is_lock_violation => "errno_is_lock_violation",
    native = Some("moonbitlang_async_errno_is_lock_violation"),
    sources = [moonbit_async:"src/fs/stub.c"];

    unsupported unsupported::i32 => "dir_is_null",
    native = Some("moonbitlang_async_dir_is_null"),
    sources = [moonbit_async:"src/fs/stub.c"];

    native fs::try_lock_file => "try_lock_file",
    native = Some("moonbitlang_async_try_lock_file"),
    sources = [moonbit_async:"src/fs/stub.c"];

    native fs::unlock_file => "unlock_file",
    native = Some("moonbitlang_async_unlock_file"),
    sources = [moonbit_async:"src/fs/stub.c"];

    support fs::get_tmp_path_len => "get_tmp_path_len",
    native = None,
    sources = [
        moonbit_async:"src/fs/stub.c",
        moonrun:"crates/moonrun/src/async_api/fs.rs"
    ];

    native fs::get_tmp_path => "get_tmp_path",
    native = Some("moonbitlang_async_get_tmp_path"),
    sources = [moonbit_async:"src/fs/stub.c"];

    native fs::dir_buffer_min_size => "dir_buffer_min_size",
    native = Some("moonbitlang_async_dir_buffer_min_size"),
    sources = [moonbit_async:"src/fs/dir.c"];

    native fs::dir_entry_length => "dir_entry_length",
    native = Some("moonbitlang_async_dir_entry_length"),
    sources = [moonbit_async:"src/fs/dir.c"];

    native fs::dir_entry_name_len => "dir_entry_get_name_len",
    native = Some("moonbitlang_async_dir_entry_get_name_len"),
    sources = [moonbit_async:"src/fs/dir.c"];

    native fs::dir_entry_name => "dir_entry_get_name",
    native = Some("moonbitlang_async_dir_entry_get_name"),
    sources = [moonbit_async:"src/fs/dir.c"];

    native fs::dir_entry_is_dir => "dir_entry_is_dir",
    native = Some("moonbitlang_async_dir_entry_is_dir"),
    sources = [moonbit_async:"src/fs/dir.c"];

    native fs::dir_entry_is_hidden => "dir_entry_is_hidden",
    native = Some("moonbitlang_async_dir_entry_is_hidden"),
    sources = [moonbit_async:"src/fs/dir.c"];

    native fs::dir_entry_file_id => "dir_entry_get_file_id",
    native = Some("moonbitlang_async_dir_entry_get_file_id"),
    sources = [moonbit_async:"src/fs/dir.c"];

    unsupported unsupported::i32 => "poll_create",
    native = Some("moonbitlang_async_poll_create"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
        moonbit_async:"src/internal/event_loop/iocp.c",
    ];

    unsupported unsupported::i32 => "poll_destroy",
    native = Some("moonbitlang_async_poll_destroy"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
        moonbit_async:"src/internal/event_loop/iocp.c",
    ];

    unsupported unsupported::i32 => "poll_register",
    native = Some("moonbitlang_async_poll_register"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
        moonbit_async:"src/internal/event_loop/iocp.c",
    ];

    unsupported unsupported::i32 => "support_wait_pid_via_poll",
    native = Some("moonbitlang_async_support_wait_pid_via_poll"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
    ];

    unsupported unsupported::i32 => "poll_register_pid",
    native = Some("moonbitlang_async_poll_register_pid"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
    ];

    unsupported unsupported::i32 => "poll_remove",
    native = Some("moonbitlang_async_poll_remove"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
    ];

    unsupported unsupported::i32 => "poll_remove_pid",
    native = Some("moonbitlang_async_poll_remove_pid"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
    ];

    unsupported unsupported::i32 => "poll_wait",
    native = Some("moonbitlang_async_poll_wait"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
        moonbit_async:"src/internal/event_loop/iocp.c",
    ];

    unsupported unsupported::i32 => "event_list_get",
    native = Some("moonbitlang_async_event_list_get"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
        moonbit_async:"src/internal/event_loop/iocp.c",
    ];

    unsupported unsupported::i32 => "event_get_fd",
    native = Some("moonbitlang_async_event_get_fd"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
        moonbit_async:"src/internal/event_loop/iocp.c",
    ];

    unsupported unsupported::i32 => "event_get_events",
    native = Some("moonbitlang_async_event_get_events"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
    ];

    unsupported unsupported::i32 => "event_get_io_result",
    native = Some("moonbitlang_async_event_get_io_result"),
    sources = [moonbit_async:"src/internal/event_loop/iocp.c"];

    unsupported unsupported::i32 => "event_get_bytes_transferred",
    native = Some("moonbitlang_async_event_get_bytes_transferred"),
    sources = [moonbit_async:"src/internal/event_loop/iocp.c"];

    unsupported unsupported::i32 => "init_WSA",
    native = Some("moonbitlang_async_init_WSA"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "cleanup_WSA",
    native = Some("moonbitlang_async_cleanup_WSA"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "make_file_io_result",
    native = Some("moonbitlang_async_make_file_io_result"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "make_socket_io_result",
    native = Some("moonbitlang_async_make_socket_io_result"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "make_socket_with_addr_io_result",
    native = Some("moonbitlang_async_make_socket_with_addr_io_result"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "make_connect_io_result",
    native = Some("moonbitlang_async_make_connect_io_result"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "make_accept_io_result",
    native = Some("moonbitlang_async_make_accept_io_result"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "make_read_dir_changes_io_result",
    native = Some("moonbitlang_async_make_read_dir_changes_io_result"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "free_io_result",
    native = Some("moonbitlang_async_free_io_result"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "io_result_get_job_id",
    native = Some("moonbitlang_async_io_result_get_job_id"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "io_result_get_status",
    native = Some("moonbitlang_async_io_result_get_status"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "cancel_io_result",
    native = Some("moonbitlang_async_cancel_io_result"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "errno_is_read_EOF",
    native = Some("moonbitlang_async_errno_is_read_EOF"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "read",
    native = Some("moonbitlang_async_read"),
    sources = [
        moonbit_async:"src/internal/event_loop/io_unix.c",
        moonbit_async:"src/internal/event_loop/io_windows.c",
    ];

    unsupported unsupported::i32 => "write",
    native = Some("moonbitlang_async_write"),
    sources = [
        moonbit_async:"src/internal/event_loop/io_unix.c",
        moonbit_async:"src/internal/event_loop/io_windows.c",
    ];

    unsupported unsupported::i32 => "connect",
    native = Some("moonbitlang_async_connect"),
    sources = [
        moonbit_async:"src/internal/event_loop/io_unix.c",
        moonbit_async:"src/internal/event_loop/io_windows.c",
    ];

    unsupported unsupported::i32 => "setup_connected_socket",
    native = Some("moonbitlang_async_setup_connected_socket"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "accept",
    native = Some("moonbitlang_async_accept"),
    sources = [
        moonbit_async:"src/internal/event_loop/io_unix.c",
        moonbit_async:"src/internal/event_loop/io_windows.c",
    ];

    unsupported unsupported::i32 => "setup_accepted_socket",
    native = Some("moonbitlang_async_setup_accepted_socket"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "get_std_handle",
    native = Some("moonbitlang_async_get_std_handle"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "read_dir_changes",
    native = Some("moonbitlang_async_read_dir_changes"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "init_thread_pool",
    native = Some("moonbitlang_async_init_thread_pool"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_open_job => "make_open_job",
    native = Some("moonbitlang_async_make_open_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::open_job_get_fd => "open_job_get_fd",
    native = Some("moonbitlang_async_open_job_get_fd"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::open_job_get_kind => "open_job_get_kind",
    native = Some("moonbitlang_async_open_job_get_kind"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::open_job_get_dev_id => "open_job_get_dev_id",
    native = Some("moonbitlang_async_open_job_get_dev_id"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::open_job_get_file_id => "open_job_get_file_id",
    native = Some("moonbitlang_async_open_job_get_file_id"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_read_job => "make_read_job",
    native = Some("moonbitlang_async_make_read_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_write_job => "make_write_job",
    native = Some("moonbitlang_async_make_write_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_file_kind_by_path_job => "make_file_kind_by_path_job",
    native = Some("moonbitlang_async_make_file_kind_by_path_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_file_size_job => "make_file_size_job",
    native = Some("moonbitlang_async_make_file_size_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::get_file_size_result => "get_file_size_result",
    native = Some("moonbitlang_async_get_file_size_result"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_file_time_job => "make_file_time_job",
    native = Some("moonbitlang_async_make_file_time_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_file_time_by_path_job => "make_file_time_by_path_job",
    native = Some("moonbitlang_async_make_file_time_by_path_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_access_job => "make_access_job",
    native = Some("moonbitlang_async_make_access_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_chmod_job => "make_chmod_job",
    native = Some("moonbitlang_async_make_chmod_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_fsync_job => "make_fsync_job",
    native = Some("moonbitlang_async_make_fsync_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_flock_job => "make_flock_job",
    native = Some("moonbitlang_async_make_flock_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_remove_job => "make_remove_job",
    native = Some("moonbitlang_async_make_remove_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_rename_job => "make_rename_job",
    native = Some("moonbitlang_async_make_rename_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_symlink_job => "make_symlink_job",
    native = Some("moonbitlang_async_make_symlink_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_mkdir_job => "make_mkdir_job",
    native = Some("moonbitlang_async_make_mkdir_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_rmdir_job => "make_rmdir_job",
    native = Some("moonbitlang_async_make_rmdir_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_readdir_job => "make_readdir_job",
    native = Some("moonbitlang_async_make_readdir_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    unsupported unsupported::i32 => "make_tcp_socket",
    native = Some("moonbitlang_async_make_tcp_socket"),
    sources = [moonbit_async:"src/socket/socket.c"];

    unsupported unsupported::i32 => "make_udp_socket",
    native = Some("moonbitlang_async_make_udp_socket"),
    sources = [moonbit_async:"src/socket/socket.c"];

    unsupported unsupported::i32 => "make_spawn_job",
    native = Some("moonbitlang_async_make_spawn_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    unsupported unsupported::i32 => "schannel_new",
    native = Some("moonbitlang_async_schannel_new"),
    sources = [moonbit_async:"src/tls/schannel.c"];

    unsupported unsupported::i32 => "tls_client_ctx",
    native = Some("moonbitlang_async_tls_client_ctx"),
    sources = [moonbit_async:"src/tls/openssl.c"];
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, fs, path::Path};

    use super::*;

    fn repo_root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("moonrun crate must live under crates/moonrun")
    }

    fn source_path(source: SourceLocation) -> std::path::PathBuf {
        match source.root {
            SourceRoot::MoonbitAsync => repo_root()
                .join("third_party/moonbitlang_async")
                .join(source.path),
            SourceRoot::Moonrun => repo_root().join(source.path),
        }
    }

    #[test]
    fn wasm_import_names_are_unique() {
        let mut seen = BTreeSet::new();
        for import in ASYNC_IMPORTS {
            assert!(
                seen.insert(import.wasm_symbol),
                "duplicate async import {}",
                import.wasm_symbol
            );
        }
    }

    #[test]
    fn native_async_symbols_map_by_stripping_namespace_prefix() {
        for import in ASYNC_IMPORTS {
            let Some(native_symbol) = import.native_symbol else {
                assert_eq!(import.kind, AsyncImportKind::WasmSupport);
                continue;
            };
            let suffix = native_symbol
                .strip_prefix(NATIVE_ASYNC_PREFIX)
                .expect("native async mapping must use the async C namespace");
            assert_eq!(import.wasm_symbol, suffix);
            assert!(!import.wasm_symbol.starts_with("async_"));
        }
    }

    #[test]
    fn declared_sources_exist_and_contain_native_symbols() {
        for import in ASYNC_IMPORTS {
            assert!(
                !import.sources.is_empty(),
                "async import {} must declare source files",
                import.wasm_symbol
            );
            for source in import.sources {
                let source_path = source_path(*source);
                let contents = fs::read_to_string(&source_path)
                    .unwrap_or_else(|error| panic!("failed to read {:?}: {error}", source_path));
                if let Some(native_symbol) = import.native_symbol {
                    assert!(
                        contents.contains(native_symbol),
                        "{:?} does not contain native symbol {} for wasm import {}",
                        source_path,
                        native_symbol,
                        import.wasm_symbol
                    );
                }
            }
        }
    }

    #[test]
    fn native_mapped_imports_have_ported_implementations() {
        let ported_symbols = crate::async_sys::ported_symbols();

        for import in ASYNC_IMPORTS {
            if import.kind != AsyncImportKind::NativeMapped {
                continue;
            }

            let native_symbol = import
                .native_symbol
                .expect("native-mapped import must declare a native symbol");
            assert!(
                import.sources.iter().any(|source| {
                    source.root == SourceRoot::MoonbitAsync
                        && ported_symbols.iter().any(|ported| {
                            ported.native_symbol == native_symbol && ported.source == source.path
                        })
                }),
                "async import {} / {} has no Rust port origin",
                import.wasm_symbol,
                native_symbol
            );
        }
    }

    #[test]
    fn ported_implementations_are_registered_imports() {
        for ported in crate::async_sys::ported_symbols() {
            assert!(
                ASYNC_IMPORTS.iter().any(|import| {
                    import.native_symbol == Some(ported.native_symbol)
                        && import.sources.iter().any(|source| {
                            source.root == SourceRoot::MoonbitAsync && source.path == ported.source
                        })
                }),
                "ported symbol {}::{} from {} / {} is not registered",
                ported.rust_module,
                ported.rust_symbol,
                ported.source,
                ported.native_symbol
            );
        }
    }
}
