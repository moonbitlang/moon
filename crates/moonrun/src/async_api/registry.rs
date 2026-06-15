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
    runtime, thread_pool, time, unsupported,
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

// This block is the complete `moonbit_v0` ABI surface registered by moonrun.
//
// Entry shape:
//   kind callback maps to "namespace/wasm_symbol",
//   native = Some("moonbitlang_async_native_symbol") | None,
//   sources = [moonbit_async:"path/in/async", moonrun:"path/in/moonrun"];
//
// Kind legend:
// - native: maps a native async C-stub symbol to the same leaf name under a
//   namespaced `moonbit_v0` field; tests require a `#[ported(...)]`
//   implementation provenance.
// - support: wasm-only support import or host-control glue. It may list a
//   native C symbol for provenance, but tests do not require a direct
//   `#[ported(...)]` implementation.
// - unsupported: declared so wasm modules link, but currently returns the
//   uniform unsupported stub.
declare_async_imports! {
    // Runtime platform and worker control.
    support runtime::exit => "runtime/exit",
    native = None,
    sources = [moonrun:"crates/moonrun/src/async_api/runtime.rs"];

    native event_loop::get_platform => "runtime/get_platform",
    native = Some("moonbitlang_async_get_platform"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native event_loop::errno_is_cancelled => "thread_pool/errno_is_cancelled",
    native = Some("moonbitlang_async_errno_is_cancelled"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::job_get_ret => "thread_pool/job_get_ret",
    native = Some("moonbitlang_async_job_get_ret"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::job_get_err => "thread_pool/job_get_err",
    native = Some("moonbitlang_async_job_get_err"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    support thread_pool::free_job => "thread_pool/free_job",
    native = None,
    sources = [moonrun:"crates/moonrun/src/async_api/thread_pool.rs"];

    support thread_pool::run_job => "thread_pool/run_job",
    native = None,
    sources = [moonrun:"crates/moonrun/src/async_api/thread_pool.rs"];

    native thread_pool::spawn_worker => "thread_pool/spawn_worker",
    native = Some("moonbitlang_async_spawn_worker"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::free_worker => "thread_pool/free_worker",
    native = Some("moonbitlang_async_free_worker"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::wake_worker => "thread_pool/wake_worker",
    native = Some("moonbitlang_async_wake_worker"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::worker_enter_idle => "thread_pool/worker_enter_idle",
    native = Some("moonbitlang_async_worker_enter_idle"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::cancel_worker => "thread_pool/cancel_worker",
    native = Some("moonbitlang_async_cancel_worker"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    support thread_pool::fetch_completion => "thread_pool/fetch_completion",
    native = Some("moonbitlang_async_fetch_completion"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_sleep_job => "thread_pool/make_sleep_job",
    native = Some("moonbitlang_async_make_sleep_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    // Process entrypoints. `spawn_process` is wasm support glue for packed argv;
    // wait still follows the native thread_pool.c job shape.
    support process::spawn_process => "process/spawn_process",
    native = None,
    sources = [moonrun:"crates/moonrun/src/async_api/process.rs"];

    native process::make_wait_for_process_job => "thread_pool/make_wait_for_process_job",
    native = Some("moonbitlang_async_make_wait_for_process_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    // Time and guest-memory helpers.
    native time::get_ms_since_epoch => "time/get_ms_since_epoch",
    native = Some("moonbitlang_async_get_ms_since_epoch"),
    sources = [moonbit_async:"src/internal/time/time.c"];

    support time::sleep_ms => "time/sleep_ms",
    native = None,
    sources = [moonbit_async:"src/internal/event_loop/event_loop.wasm.mbt"];

    support memory::copy_from_guest => "memory/copy_from_guest",
    native = None,
    sources = [moonrun:"crates/moonrun/src/async_host/mod.rs"];

    support memory::zero_guest => "memory/zero_guest",
    native = None,
    sources = [moonrun:"crates/moonrun/src/async_host/mod.rs"];

    // os_error/stub.c predicates and errno accessors.
    native os_error::get_errno => "os_error/get_errno",
    native = Some("moonbitlang_async_get_errno"),
    sources = [moonbit_async:"src/os_error/stub.c"];

    native os_error::is_nonblocking_io_error => "os_error/is_nonblocking_io_error",
    native = Some("moonbitlang_async_is_nonblocking_io_error"),
    sources = [moonbit_async:"src/os_error/stub.c"];

    native os_error::is_eintr => "os_error/is_EINTR",
    native = Some("moonbitlang_async_is_EINTR"),
    sources = [moonbit_async:"src/os_error/stub.c"];

    native os_error::is_enoent => "os_error/is_ENOENT",
    native = Some("moonbitlang_async_is_ENOENT"),
    sources = [moonbit_async:"src/os_error/stub.c"];

    native os_error::is_eexist => "os_error/is_EEXIST",
    native = Some("moonbitlang_async_is_EEXIST"),
    sources = [moonbit_async:"src/os_error/stub.c"];

    native os_error::is_eacces => "os_error/is_EACCES",
    native = Some("moonbitlang_async_is_EACCES"),
    sources = [moonbit_async:"src/os_error/stub.c"];

    native os_error::is_econnrefused => "os_error/is_ECONNREFUSED",
    native = Some("moonbitlang_async_is_ECONNREFUSED"),
    sources = [moonbit_async:"src/os_error/stub.c"];

    native os_error::is_error_notify_enum_dir => "os_error/is_ERROR_NOTIFY_ENUM_DIR",
    native = Some("moonbitlang_async_is_ERROR_NOTIFY_ENUM_DIR"),
    sources = [moonbit_async:"src/os_error/stub.c"];

    native os_error::get_enotdir => "os_error/get_ENOTDIR",
    native = Some("moonbitlang_async_get_ENOTDIR"),
    sources = [moonbit_async:"src/os_error/stub.c"];

    // internal/fd_util/stub.c. Raw-fd mutation helpers are present but
    // unsupported because wasm async uses host handles first.
    unsupported unsupported::i32 => "fd_util/get_invalid_handle",
    native = Some("moonbitlang_async_get_invalid_handle"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    native fs::close_fd => "fd_util/close_fd",
    native = Some("moonbitlang_async_close_fd"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    unsupported unsupported::i32 => "fd_util/fd_is_nonblocking",
    native = Some("moonbitlang_async_fd_is_nonblocking"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    unsupported unsupported::i32 => "fd_util/set_blocking",
    native = Some("moonbitlang_async_set_blocking"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    unsupported unsupported::i32 => "fd_util/set_nonblocking",
    native = Some("moonbitlang_async_set_nonblocking"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    unsupported unsupported::i32 => "fd_util/set_cloexec",
    native = Some("moonbitlang_async_set_cloexec"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    unsupported unsupported::i32 => "fd_util/create_named_pipe_server",
    native = Some("moonbitlang_async_create_named_pipe_server"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    unsupported unsupported::i32 => "fd_util/create_named_pipe_client",
    native = Some("moonbitlang_async_create_named_pipe_client"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    native fd_util::pipe => "fd_util/pipe",
    native = Some("moonbitlang_async_pipe"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    native fd_util::sizeof_file_time => "fd_util/sizeof_file_time",
    native = Some("moonbitlang_async_sizeof_file_time"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    native fd_util::get_atime_sec => "fd_util/get_atime_sec",
    native = Some("moonbitlang_async_get_atime_sec"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    native fd_util::get_atime_nsec => "fd_util/get_atime_nsec",
    native = Some("moonbitlang_async_get_atime_nsec"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    native fd_util::get_mtime_sec => "fd_util/get_mtime_sec",
    native = Some("moonbitlang_async_get_mtime_sec"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    native fd_util::get_mtime_nsec => "fd_util/get_mtime_nsec",
    native = Some("moonbitlang_async_get_mtime_nsec"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    native fd_util::get_ctime_sec => "fd_util/get_ctime_sec",
    native = Some("moonbitlang_async_get_ctime_sec"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    native fd_util::get_ctime_nsec => "fd_util/get_ctime_nsec",
    native = Some("moonbitlang_async_get_ctime_nsec"),
    sources = [moonbit_async:"src/internal/fd_util/stub.c"];

    // Small internal utility stubs.
    native env_util::getpid => "env_util/getpid",
    native = Some("moonbitlang_async_getpid"),
    sources = [moonbit_async:"src/internal/env_util/stub.c"];

    native c_buffer::blit_to_c => "c_buffer/blit_to_c",
    native = Some("moonbitlang_async_blit_to_c"),
    sources = [moonbit_async:"src/internal/c_buffer/stub.c"];

    native c_buffer::blit_from_c => "c_buffer/blit_from_c",
    native = Some("moonbitlang_async_blit_from_c"),
    sources = [moonbit_async:"src/internal/c_buffer/stub.c"];

    native c_buffer::c_buffer_get => "c_buffer/c_buffer_get",
    native = Some("moonbitlang_async_c_buffer_get"),
    sources = [moonbit_async:"src/internal/c_buffer/stub.c"];

    native c_buffer::strlen => "c_buffer/strlen",
    native = Some("moonbitlang_async_strlen"),
    sources = [moonbit_async:"src/internal/c_buffer/stub.c"];

    native c_buffer::null_pointer => "c_buffer/null_pointer",
    native = Some("moonbitlang_async_null_pointer"),
    sources = [moonbit_async:"src/internal/c_buffer/stub.c"];

    native c_buffer::pointer_is_null => "c_buffer/pointer_is_null",
    native = Some("moonbitlang_async_pointer_is_null"),
    sources = [moonbit_async:"src/internal/c_buffer/stub.c"];

    unsupported unsupported::i32 => "os_string/c_buffer_as_string",
    native = Some("moonbitlang_async_c_buffer_as_string"),
    sources = [moonbit_async:"src/internal/os_string/stub.c"];

    // fs/stub.c and fs/dir.c.
    native fs::errno_is_lock_violation => "fs/errno_is_lock_violation",
    native = Some("moonbitlang_async_errno_is_lock_violation"),
    sources = [moonbit_async:"src/fs/stub.c"];

    unsupported unsupported::i32 => "fs/dir_is_null",
    native = Some("moonbitlang_async_dir_is_null"),
    sources = [moonbit_async:"src/fs/stub.c"];

    native fs::try_lock_file => "fs/try_lock_file",
    native = Some("moonbitlang_async_try_lock_file"),
    sources = [moonbit_async:"src/fs/stub.c"];

    native fs::unlock_file => "fs/unlock_file",
    native = Some("moonbitlang_async_unlock_file"),
    sources = [moonbit_async:"src/fs/stub.c"];

    // Returns the UTF-16 code-unit length that the guest must allocate for
    // `fs/get_tmp_path`.
    support fs::get_tmp_path_len => "fs/get_tmp_path_len",
    native = None,
    sources = [
        moonbit_async:"src/fs/stub.c",
        moonrun:"crates/moonrun/src/async_api/fs.rs"
    ];

    // Writes the native temporary directory as UTF-16 code units into a
    // guest-allocated MoonBit String.
    native fs::get_tmp_path => "fs/get_tmp_path",
    native = Some("moonbitlang_async_get_tmp_path"),
    sources = [
        moonbit_async:"src/fs/stub.c",
        moonrun:"crates/moonrun/src/async_sys/fs/stub.rs"
    ];

    native fs::dir_buffer_min_size => "fs/dir_buffer_min_size",
    native = Some("moonbitlang_async_dir_buffer_min_size"),
    sources = [moonbit_async:"src/fs/dir.c"];

    native fs::dir_entry_length => "fs/dir_entry_length",
    native = Some("moonbitlang_async_dir_entry_length"),
    sources = [moonbit_async:"src/fs/dir.c"];

    native fs::dir_entry_name_len => "fs/dir_entry_get_name_len",
    native = Some("moonbitlang_async_dir_entry_get_name_len"),
    sources = [moonbit_async:"src/fs/dir.c"];

    native fs::dir_entry_name => "fs/dir_entry_get_name",
    native = Some("moonbitlang_async_dir_entry_get_name"),
    sources = [moonbit_async:"src/fs/dir.c"];

    native fs::dir_entry_is_dir => "fs/dir_entry_is_dir",
    native = Some("moonbitlang_async_dir_entry_is_dir"),
    sources = [moonbit_async:"src/fs/dir.c"];

    native fs::dir_entry_is_hidden => "fs/dir_entry_is_hidden",
    native = Some("moonbitlang_async_dir_entry_is_hidden"),
    sources = [moonbit_async:"src/fs/dir.c"];

    native fs::dir_entry_file_id => "fs/dir_entry_get_file_id",
    native = Some("moonbitlang_async_dir_entry_get_file_id"),
    sources = [moonbit_async:"src/fs/dir.c"];

    // Poller entrypoints are registered for link compatibility. The current
    // wasm event loop slice uses worker completions and timers, not native
    // epoll/kqueue/IOCP polling.
    unsupported unsupported::i32 => "poll/poll_create",
    native = Some("moonbitlang_async_poll_create"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
        moonbit_async:"src/internal/event_loop/iocp.c",
    ];

    unsupported unsupported::i32 => "poll/poll_destroy",
    native = Some("moonbitlang_async_poll_destroy"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
        moonbit_async:"src/internal/event_loop/iocp.c",
    ];

    unsupported unsupported::i32 => "poll/poll_register",
    native = Some("moonbitlang_async_poll_register"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
        moonbit_async:"src/internal/event_loop/iocp.c",
    ];

    unsupported unsupported::i32 => "poll/support_wait_pid_via_poll",
    native = Some("moonbitlang_async_support_wait_pid_via_poll"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
    ];

    unsupported unsupported::i32 => "poll/poll_register_pid",
    native = Some("moonbitlang_async_poll_register_pid"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
    ];

    unsupported unsupported::i32 => "poll/poll_remove",
    native = Some("moonbitlang_async_poll_remove"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
    ];

    unsupported unsupported::i32 => "poll/poll_remove_pid",
    native = Some("moonbitlang_async_poll_remove_pid"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
    ];

    unsupported unsupported::i32 => "poll/poll_wait",
    native = Some("moonbitlang_async_poll_wait"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
        moonbit_async:"src/internal/event_loop/iocp.c",
    ];

    unsupported unsupported::i32 => "poll/event_list_get",
    native = Some("moonbitlang_async_event_list_get"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
        moonbit_async:"src/internal/event_loop/iocp.c",
    ];

    unsupported unsupported::i32 => "poll/event_get_fd",
    native = Some("moonbitlang_async_event_get_fd"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
        moonbit_async:"src/internal/event_loop/iocp.c",
    ];

    unsupported unsupported::i32 => "poll/event_get_events",
    native = Some("moonbitlang_async_event_get_events"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
    ];

    unsupported unsupported::i32 => "poll/event_get_io_result",
    native = Some("moonbitlang_async_event_get_io_result"),
    sources = [moonbit_async:"src/internal/event_loop/iocp.c"];

    unsupported unsupported::i32 => "poll/event_get_bytes_transferred",
    native = Some("moonbitlang_async_event_get_bytes_transferred"),
    sources = [moonbit_async:"src/internal/event_loop/iocp.c"];

    // Direct IO and Windows IO-result APIs are outside the current MVP.
    unsupported unsupported::i32 => "io_windows/init_WSA",
    native = Some("moonbitlang_async_init_WSA"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "io_windows/cleanup_WSA",
    native = Some("moonbitlang_async_cleanup_WSA"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "io_windows/make_file_io_result",
    native = Some("moonbitlang_async_make_file_io_result"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "io_windows/make_socket_io_result",
    native = Some("moonbitlang_async_make_socket_io_result"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "io_windows/make_socket_with_addr_io_result",
    native = Some("moonbitlang_async_make_socket_with_addr_io_result"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "io_windows/make_connect_io_result",
    native = Some("moonbitlang_async_make_connect_io_result"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "io_windows/make_accept_io_result",
    native = Some("moonbitlang_async_make_accept_io_result"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "io_windows/make_read_dir_changes_io_result",
    native = Some("moonbitlang_async_make_read_dir_changes_io_result"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "io_windows/free_io_result",
    native = Some("moonbitlang_async_free_io_result"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "io_windows/io_result_get_job_id",
    native = Some("moonbitlang_async_io_result_get_job_id"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "io_windows/io_result_get_status",
    native = Some("moonbitlang_async_io_result_get_status"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "io_windows/cancel_io_result",
    native = Some("moonbitlang_async_cancel_io_result"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "io_windows/errno_is_read_EOF",
    native = Some("moonbitlang_async_errno_is_read_EOF"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "io/read",
    native = Some("moonbitlang_async_read"),
    sources = [
        moonbit_async:"src/internal/event_loop/io_unix.c",
        moonbit_async:"src/internal/event_loop/io_windows.c",
    ];

    unsupported unsupported::i32 => "io/write",
    native = Some("moonbitlang_async_write"),
    sources = [
        moonbit_async:"src/internal/event_loop/io_unix.c",
        moonbit_async:"src/internal/event_loop/io_windows.c",
    ];

    unsupported unsupported::i32 => "io/connect",
    native = Some("moonbitlang_async_connect"),
    sources = [
        moonbit_async:"src/internal/event_loop/io_unix.c",
        moonbit_async:"src/internal/event_loop/io_windows.c",
    ];

    unsupported unsupported::i32 => "io_windows/setup_connected_socket",
    native = Some("moonbitlang_async_setup_connected_socket"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "io/accept",
    native = Some("moonbitlang_async_accept"),
    sources = [
        moonbit_async:"src/internal/event_loop/io_unix.c",
        moonbit_async:"src/internal/event_loop/io_windows.c",
    ];

    unsupported unsupported::i32 => "io_windows/setup_accepted_socket",
    native = Some("moonbitlang_async_setup_accepted_socket"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "io_windows/get_std_handle",
    native = Some("moonbitlang_async_get_std_handle"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "io_windows/read_dir_changes",
    native = Some("moonbitlang_async_read_dir_changes"),
    sources = [moonbit_async:"src/internal/event_loop/io_windows.c"];

    unsupported unsupported::i32 => "thread_pool/init_thread_pool",
    native = Some("moonbitlang_async_init_thread_pool"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    // thread_pool.c FS jobs. Path-taking jobs use the Guest String Path ABI:
    // MoonBit String pointer plus UTF-16 code-unit length.
    native thread_pool::make_open_job => "thread_pool/make_open_job",
    native = Some("moonbitlang_async_make_open_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::open_job_get_fd => "thread_pool/open_job_get_fd",
    native = Some("moonbitlang_async_open_job_get_fd"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::open_job_get_kind => "thread_pool/open_job_get_kind",
    native = Some("moonbitlang_async_open_job_get_kind"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::open_job_get_dev_id => "thread_pool/open_job_get_dev_id",
    native = Some("moonbitlang_async_open_job_get_dev_id"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::open_job_get_file_id => "thread_pool/open_job_get_file_id",
    native = Some("moonbitlang_async_open_job_get_file_id"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_read_job => "thread_pool/make_read_job",
    native = Some("moonbitlang_async_make_read_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_write_job => "thread_pool/make_write_job",
    native = Some("moonbitlang_async_make_write_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_file_kind_by_path_job => "thread_pool/make_file_kind_by_path_job",
    native = Some("moonbitlang_async_make_file_kind_by_path_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_file_size_job => "thread_pool/make_file_size_job",
    native = Some("moonbitlang_async_make_file_size_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::get_file_size_result => "thread_pool/get_file_size_result",
    native = Some("moonbitlang_async_get_file_size_result"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_file_time_job => "thread_pool/make_file_time_job",
    native = Some("moonbitlang_async_make_file_time_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_file_time_by_path_job => "thread_pool/make_file_time_by_path_job",
    native = Some("moonbitlang_async_make_file_time_by_path_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_access_job => "thread_pool/make_access_job",
    native = Some("moonbitlang_async_make_access_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_chmod_job => "thread_pool/make_chmod_job",
    native = Some("moonbitlang_async_make_chmod_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_fsync_job => "thread_pool/make_fsync_job",
    native = Some("moonbitlang_async_make_fsync_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_flock_job => "thread_pool/make_flock_job",
    native = Some("moonbitlang_async_make_flock_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_remove_job => "thread_pool/make_remove_job",
    native = Some("moonbitlang_async_make_remove_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_rename_job => "thread_pool/make_rename_job",
    native = Some("moonbitlang_async_make_rename_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_symlink_job => "thread_pool/make_symlink_job",
    native = Some("moonbitlang_async_make_symlink_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_mkdir_job => "thread_pool/make_mkdir_job",
    native = Some("moonbitlang_async_make_mkdir_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_rmdir_job => "thread_pool/make_rmdir_job",
    native = Some("moonbitlang_async_make_rmdir_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    native thread_pool::make_readdir_job => "thread_pool/make_readdir_job",
    native = Some("moonbitlang_async_make_readdir_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    // Sockets, spawn jobs, and TLS remain deferred; they are registered as
    // unsupported imports so wasm modules fail at the operation boundary rather
    // than at instantiation.
    unsupported unsupported::i32 => "socket/make_tcp_socket",
    native = Some("moonbitlang_async_make_tcp_socket"),
    sources = [moonbit_async:"src/socket/socket.c"];

    unsupported unsupported::i32 => "socket/make_udp_socket",
    native = Some("moonbitlang_async_make_udp_socket"),
    sources = [moonbit_async:"src/socket/socket.c"];

    unsupported unsupported::i32 => "thread_pool/make_spawn_job",
    native = Some("moonbitlang_async_make_spawn_job"),
    sources = [moonbit_async:"src/internal/event_loop/thread_pool.c"];

    unsupported unsupported::i32 => "tls/schannel_new",
    native = Some("moonbitlang_async_schannel_new"),
    sources = [moonbit_async:"src/tls/schannel.c"];

    unsupported unsupported::i32 => "tls/tls_client_ctx",
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
    fn runtime_exit_is_part_of_moonbit_v0() {
        assert!(
            ASYNC_IMPORTS.iter().any(|import| {
                import.kind == AsyncImportKind::WasmSupport
                    && import.wasm_symbol == "runtime/exit"
                    && import.native_symbol.is_none()
            }),
            "async wasm integration must not depend on older runtime namespaces for exit"
        );
    }

    #[test]
    fn wasm_import_names_are_namespaced_and_keep_native_leaf_names() {
        for import in ASYNC_IMPORTS {
            let Some((namespace, leaf)) = import.wasm_symbol.split_once('/') else {
                panic!("async import {} must be namespaced", import.wasm_symbol);
            };
            assert!(
                !namespace.is_empty(),
                "empty namespace for {}",
                import.wasm_symbol
            );
            assert!(
                !leaf.is_empty(),
                "empty leaf name for {}",
                import.wasm_symbol
            );
            assert!(
                !leaf.contains('/'),
                "async import {} must use exactly one namespace separator",
                import.wasm_symbol
            );

            let Some(native_symbol) = import.native_symbol else {
                assert_eq!(import.kind, AsyncImportKind::WasmSupport);
                continue;
            };
            let suffix = native_symbol
                .strip_prefix(NATIVE_ASYNC_PREFIX)
                .expect("native async mapping must use the async C namespace");
            assert_eq!(leaf, suffix);
            assert!(!leaf.starts_with("async_"));
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
