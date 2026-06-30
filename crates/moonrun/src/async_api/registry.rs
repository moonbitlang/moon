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

use super::context::{
    AsyncContext, FinishI32, FinishI64, FinishVoid, ImportArgs, ImportContext, callback_context,
    throw_import_error,
};
#[cfg(test)]
use super::provenance::{PortedImport, SourceLocation, SourceRoot};
use super::{
    c_buffer, env_util, event_bus, event_loop, fd_util, fs, io, os_error, os_string, runtime,
    socket, thread_pool, time,
};

pub(crate) const MOONBIT_ASYNC_MODULE: &str = "moonbitlang/async";
#[cfg(test)]
const NATIVE_ASYNC_PREFIX: &str = "moonbitlang_async_";

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AsyncImportKind {
    Ported,
    Helper,
    Fake,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WasmType {
    I32,
    I64,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AsyncImport {
    kind: AsyncImportKind,
    callback_module: &'static str,
    callback_symbol: &'static str,
    wasm_symbol: &'static str,
    params: &'static [WasmType],
    result: Option<WasmType>,
}

#[cfg(test)]
macro_rules! import_kind {
    (ported) => {
        AsyncImportKind::Ported
    };
    (helper) => {
        AsyncImportKind::Helper
    };
    (fake) => {
        AsyncImportKind::Fake
    };
}

#[cfg(test)]
macro_rules! wasm_type {
    (i32) => {
        WasmType::I32
    };
    (i64) => {
        WasmType::I64
    };
    (u64) => {
        WasmType::I64
    };
}

#[cfg(test)]
macro_rules! wasm_result {
    (void) => {
        None
    };
    ($ty:ident) => {
        Some(wasm_type!($ty))
    };
}

macro_rules! decode_wasm_arg {
    ($args:ident, i32) => {
        $args.next_i32()
    };
    ($args:ident, i64) => {
        $args.next_i64()
    };
    ($args:ident, u64) => {
        $args.next_u64()
    };
}

macro_rules! decode_wasm_args {
    ($scope:ident, $args:ident,) => {
        Ok(())
    };
    ($scope:ident, $args:ident, $($arg:ident : $arg_ty:ident),+ $(,)?) => {{
        let mut import_args = ImportArgs::new($scope, &$args);
        let decoded_args: crate::async_host::AsyncHostResult<_> = (|| {
            $(
                let $arg = decode_wasm_arg!(import_args, $arg_ty)?;
            )*
            Ok(($($arg,)*))
        })();
        decoded_args
    }};
}

macro_rules! finish_wasm_import {
    ($scope:ident, $ret:ident, $name:expr, void, $result:expr) => {
        $result.finish_void($scope, &mut $ret, $name)
    };
    ($scope:ident, $ret:ident, $name:expr, i32, $result:expr) => {
        $result.finish_i32($scope, &mut $ret, $name)
    };
    ($scope:ident, $ret:ident, $name:expr, i64, $result:expr) => {
        $result.finish_i64($scope, &mut $ret, $name)
    };
    ($scope:ident, $ret:ident, $name:expr, u64, $result:expr) => {
        $result.finish_i64($scope, &mut $ret, $name)
    };
}

macro_rules! declare_async_imports {
    ($(
        $(#[$meta:meta])*
        $kind:ident $module:ident::$callback:ident (
            $($arg:ident : $arg_ty:ident),* $(,)?
        ) -> $ret_ty:ident => $wasm_symbol:literal;
    )*) => {
        #[cfg(test)]
        const ASYNC_IMPORTS: &[AsyncImport] = &[
            $(
                $(#[$meta])*
                AsyncImport {
                    kind: import_kind!($kind),
                    callback_module: stringify!($module),
                    callback_symbol: stringify!($callback),
                    wasm_symbol: $wasm_symbol,
                    params: &[$(wasm_type!($arg_ty)),*],
                    result: wasm_result!($ret_ty),
                },
            )*
        ];

        pub(super) fn register_imports<'s>(
            obj: v8::Local<'s, v8::Object>,
            scope: &mut v8::HandleScope<'s>,
            context_ptr: *const AsyncContext,
        ) {
            $(
                $(#[$meta])*
                register_async_import!(
                    $kind,
                    obj,
                    scope,
                    context_ptr,
                    $wasm_symbol,
                    $ret_ty,
                    $module::$callback,
                    ($($arg : $arg_ty),*)
                );
            )*
        }
    };
}

macro_rules! register_async_import {
    (
        fake,
        $obj:ident,
        $scope:ident,
        $context_ptr:ident,
        $wasm_symbol:literal,
        $ret_ty:ident,
        $module:ident::$callback:ident,
        ($($arg:ident : $arg_ty:ident),* $(,)?)
    ) => {{
        fn callback(
            _scope: &mut v8::HandleScope,
            _args: v8::FunctionCallbackArguments,
            _ret: v8::ReturnValue,
        ) {
            unreachable!("fake async import should not be called")
        }
        register_func_impl($obj, $scope, $wasm_symbol, callback, $context_ptr);
    }};
    (
        $kind:ident,
        $obj:ident,
        $scope:ident,
        $context_ptr:ident,
        $wasm_symbol:literal,
        $ret_ty:ident,
        $module:ident::$callback:ident,
        ($($arg:ident : $arg_ty:ident),* $(,)?)
    ) => {{
        fn callback(
            scope: &mut v8::HandleScope,
            args: v8::FunctionCallbackArguments,
            mut ret: v8::ReturnValue,
        ) {
            let _ = &args;
            let host_context = callback_context(&args);
            let decoded_args: crate::async_host::AsyncHostResult<_> =
                decode_wasm_args!(scope, args, $($arg : $arg_ty),*);
            match decoded_args {
                Ok(($($arg,)*)) => {
                    let result = {
                        let mut context = ImportContext::new(scope, host_context);
                        $module::$callback(&mut context, $($arg),*)
                    };
                    finish_wasm_import!(
                        scope,
                        ret,
                        $wasm_symbol,
                        $ret_ty,
                        result
                    );
                }
                Err(error) => throw_import_error(scope, $wasm_symbol, error),
            }
        }
        register_func_impl($obj, $scope, $wasm_symbol, callback, $context_ptr);
    }};
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

// This block is the complete `moonbitlang/async` ABI surface registered by moonrun.
//
// Entry shape:
//   kind callback maps to "namespace/wasm_symbol".
//
// Kind legend:
// - ported: imports that have Rust ports corresponding to native async C stubs.
//   Tests require separate native provenance entries for these imports.
// - helper: wasm-only support import or host-control glue.
// - fake: link-only import for runtime-dispatched wasm; the generated callback is unreachable.
declare_async_imports! {

    // Runtime platform and worker control.
    helper runtime::exit(code: i32) -> void => "runtime/exit";

    ported event_loop::get_platform() -> i32 => "runtime/get_platform";

    #[cfg(windows)]
    ported event_loop::init_wsa() -> i32 => "runtime/init_WSA";

    #[cfg(not(windows))]
    fake event_loop::init_wsa() -> i32 => "runtime/init_WSA";

    #[cfg(windows)]
    ported event_loop::cleanup_wsa() -> i32 => "runtime/cleanup_WSA";

    #[cfg(not(windows))]
    fake event_loop::cleanup_wsa() -> i32 => "runtime/cleanup_WSA";

    ported event_bus::create() -> u64 => "event_bus/create";

    ported event_bus::destroy(bus: u64) -> void => "event_bus/destroy";

    ported event_bus::register(
        bus: u64,
        fd: u64,
        read_only: i32,
    ) -> i32 => "event_bus/register";

    ported event_bus::wait(bus: u64, timeout_ms: i32) -> i32 => "event_bus/wait";

    ported event_bus::get_event(bus: u64, index: i32) -> u64 => "event_bus/get_event";

    ported event_bus::event_fd(event: u64) -> u64 => "event_bus/event_fd";

    #[cfg(unix)]
    ported event_bus::event_events(event: u64) -> i32 => "event_bus/event_events/unix";

    #[cfg(windows)]
    fake event_bus::event_events(event: u64) -> i32 => "event_bus/event_events/unix";

    #[cfg(windows)]
    ported event_bus::event_io_result(event: u64) -> u64 => "event_bus/event_io_result/windows";

    #[cfg(not(windows))]
    fake event_bus::event_io_result(event: u64) -> u64 => "event_bus/event_io_result/windows";

    #[cfg(windows)]
    ported event_bus::event_bytes_transferred(event: u64) -> i32 => "event_bus/event_bytes_transferred/windows";

    #[cfg(not(windows))]
    fake event_bus::event_bytes_transferred(event: u64) -> i32 => "event_bus/event_bytes_transferred/windows";

    ported event_loop::errno_is_cancelled(errno: i32) -> i32 => "thread_pool/errno_is_cancelled";

    ported thread_pool::job_get_ret(job: u64) -> i32 => "thread_pool/job_get_ret";

    ported thread_pool::job_get_err(job: u64) -> i32 => "thread_pool/job_get_err";

    helper thread_pool::free_job(job: u64) -> void => "thread_pool/free_job";

    helper thread_pool::run_job(job: u64) -> void => "thread_pool/run_job";

    ported thread_pool::spawn_worker(completion_id: i32, job: u64) -> u64 => "thread_pool/spawn_worker";

    ported thread_pool::free_worker(worker: u64) -> void => "thread_pool/free_worker";

    ported thread_pool::wake_worker(worker: u64, completion_id: i32, job: u64) -> void => "thread_pool/wake_worker";

    ported thread_pool::worker_enter_idle(worker: u64) -> void => "thread_pool/worker_enter_idle";

    ported thread_pool::cancel_worker(worker: u64) -> i32 => "thread_pool/cancel_worker";

    helper thread_pool::init_thread_pool(poll: u64) -> u64 => "thread_pool/init_thread_pool";

    helper thread_pool::destroy_thread_pool() -> void => "thread_pool/destroy_thread_pool";

    #[cfg(unix)]
    helper thread_pool::fetch_completion(source_fd: u64, dst: i32, max_jobs: i32) -> i32 => "thread_pool/fetch_completion/unix";

    #[cfg(windows)]
    fake thread_pool::fetch_completion(source_fd: u64, dst: i32, max_jobs: i32) -> i32 => "thread_pool/fetch_completion/unix";

    ported thread_pool::make_sleep_job(duration_ms: i32) -> u64 => "thread_pool/make_sleep_job";

    // Time helpers.
    helper time::get_ms_since_epoch() -> u64 => "time/get_ms_since_epoch";

    // os_error/stub.c predicates, errno accessors, and string formatting.
    ported os_error::get_errno() -> i32 => "os_error/get_errno";

    ported os_error::is_nonblocking_io_error(errno: i32) -> i32 => "os_error/is_nonblocking_io_error";

    ported os_error::is_eintr(errno: i32) -> i32 => "os_error/is_EINTR";

    ported os_error::is_enoent(errno: i32) -> i32 => "os_error/is_ENOENT";

    ported os_error::is_eexist(errno: i32) -> i32 => "os_error/is_EEXIST";

    ported os_error::is_eacces(errno: i32) -> i32 => "os_error/is_EACCES";

    ported os_error::is_econnrefused(errno: i32) -> i32 => "os_error/is_ECONNREFUSED";

    ported os_error::is_error_notify_enum_dir(errno: i32) -> i32 => "os_error/is_ERROR_NOTIFY_ENUM_DIR";

    ported os_error::get_enotdir() -> i32 => "os_error/get_ENOTDIR";

    ported os_error::errno_to_string(errno: i32) -> u64 => "os_error/errno_to_string";

    helper os_error::free_errno_str(ptr: u64) -> void => "os_error/free_errno_str";

    // Decode host-native strings into guest-owned MoonBit String storage.
    helper os_string::decode_len(ptr: u64, offset: i32, len: i32) -> i32 => "os_string/decode_len";

    helper os_string::decode(ptr: u64, offset: i32, len: i32, out: i32, out_len: i32) -> void => "os_string/decode";

    helper fs::close_fd(fd: u64) -> i32 => "fd_util/close_fd";

    helper fd_util::invalid_fd() -> u64 => "fd_util/invalid_fd";

    #[cfg(unix)]
    ported fd_util::pipe(
        dst: i32,
        len: i32,
        read_end_is_async: i32,
        write_end_is_async: i32,
    ) -> i32 => "fd_util/pipe";

    #[cfg(windows)]
    helper fd_util::pipe(
        dst: i32,
        len: i32,
        read_end_is_async: i32,
        write_end_is_async: i32,
    ) -> i32 => "fd_util/pipe";

    #[cfg(unix)]
    ported fd_util::set_nonblocking(fd: u64) -> i32 => "fd_util/set_nonblocking/unix";

    #[cfg(windows)]
    fake fd_util::set_nonblocking(fd: u64) -> i32 => "fd_util/set_nonblocking/unix";

    helper fd_util::set_cloexec(fd: u64) -> i32 => "fd_util/set_cloexec";

    helper fd_util::sizeof_file_time() -> i32 => "fd_util/sizeof_file_time";

    ported fd_util::get_atime_sec(ptr: i32) -> i64 => "fd_util/get_atime_sec";

    ported fd_util::get_atime_nsec(ptr: i32) -> i32 => "fd_util/get_atime_nsec";

    ported fd_util::get_mtime_sec(ptr: i32) -> i64 => "fd_util/get_mtime_sec";

    ported fd_util::get_mtime_nsec(ptr: i32) -> i32 => "fd_util/get_mtime_nsec";

    ported fd_util::get_ctime_sec(ptr: i32) -> i64 => "fd_util/get_ctime_sec";

    ported fd_util::get_ctime_nsec(ptr: i32) -> i32 => "fd_util/get_ctime_nsec";

    // Small internal utility stubs.
    ported env_util::getpid() -> i32 => "env_util/getpid";

    #[cfg(unix)]
    ported io::read(fd: u64, dst: i32, offset: i32, len: i32) -> i32 => "io/read/unix";

    #[cfg(windows)]
    fake io::read(fd: u64, dst: i32, offset: i32, len: i32) -> i32 => "io/read/unix";

    #[cfg(unix)]
    ported io::write(fd: u64, src: i32, offset: i32, len: i32) -> i32 => "io/write/unix";

    #[cfg(windows)]
    fake io::write(fd: u64, src: i32, offset: i32, len: i32) -> i32 => "io/write/unix";

    ported socket::ipv4_addr_size() -> i32 => "socket/ipv4_addr_size";

    ported socket::ipv6_addr_size() -> i32 => "socket/ipv6_addr_size";

    ported socket::init_ip_addr(addr: i32, ip: i32, port: i32) -> void => "socket/init_ip_addr";

    ported socket::init_ipv6_addr(addr: i32, ip: i32, port: i32, scope_id: i32) -> void => "socket/init_ipv6_addr";

    #[cfg(unix)]
    ported socket::gai_strerror(code: i32) -> u64 => "socket/gai_strerror";

    #[cfg(windows)]
    fake socket::gai_strerror(code: i32) -> u64 => "socket/gai_strerror";

    ported socket::ip_addr_get_ip(addr: i32, addr_len: i32) -> i32 => "socket/ip_addr_get_ip";

    ported socket::ip_addr_get_port(addr: i32, addr_len: i32) -> i32 => "socket/ip_addr_get_port";

    ported socket::addr_is_ipv6(addr: i32, addr_len: i32) -> i32 => "socket/addr_is_ipv6";

    ported socket::addr_is_multicast(addr: i32, addr_len: i32) -> i32 => "socket/addr_is_multicast";

    ported socket::addr_copy_ipv6_bytes(addr: i32, out: i32, addr_len: i32, len: i32) -> void => "socket/addr_copy_ipv6_bytes";

    ported socket::addr_get_ipv6_scope_id(addr: i32, addr_len: i32) -> i32 => "socket/addr_get_ipv6_scope_id";

    ported socket::addr_is_ipv6_wildcard(addr: i32, addr_len: i32) -> i32 => "socket/addr_is_ipv6_wildcard";

    helper socket::addrinfo_is_null(addrinfo: u64) -> i32 => "socket/addrinfo_is_null";

    helper socket::addrinfo_get_next(addrinfo: u64) -> u64 => "socket/addrinfo_get_next";

    ported socket::addrinfo_addr_size(addrinfo: u64) -> i32 => "socket/addrinfo_addr_size";

    ported socket::addrinfo_fill_addr(addrinfo: u64, out: i32, port: i32, out_len: i32) -> void => "socket/addrinfo_fill_addr";

    helper socket::addrinfo_free(addrinfo: u64) -> void => "socket/addrinfo_free";

    ported socket::make_tcp_socket(family: i32) -> u64 => "socket/make_tcp_socket";

    ported socket::make_udp_socket(family: i32, multicast: i32) -> u64 => "socket/make_udp_socket";

    ported socket::join_multicast_group(fd: u64, multi_addr: i32, local_addr: i32, multi_addr_len: i32, local_addr_len: i32) -> i32 => "socket/join_multicast_group";

    ported socket::join_multicast_group_v6(fd: u64, multi_addr: i32, interface_index: i32, multi_addr_len: i32) -> i32 => "socket/join_multicast_group_v6";

    ported socket::set_multicast_interface(fd: u64, local_addr: i32, local_addr_len: i32) -> i32 => "socket/set_multicast_interface";

    ported socket::set_multicast_interface_v6(fd: u64, interface_index: i32) -> i32 => "socket/set_multicast_interface_v6";

    ported socket::set_multicast_ttl(fd: u64, ttl: i32, family: i32) -> i32 => "socket/set_multicast_ttl";

    ported socket::set_multicast_loopback(fd: u64, enable: i32, family: i32) -> i32 => "socket/set_multicast_loopback";

    ported socket::disable_nagle(fd: u64) -> i32 => "socket/disable_nagle";

    ported socket::allow_reuse_addr(fd: u64) -> i32 => "socket/allow_reuse_addr";

    ported socket::set_ipv6_only(fd: u64, ipv6_only: i32) -> i32 => "socket/set_ipv6_only";

    ported socket::listen(fd: u64) -> i32 => "socket/listen";

    ported socket::enable_keepalive(fd: u64, keep_idle: i32, keep_count: i32, keep_intvl: i32) -> i32 => "socket/enable_keepalive";

    ported socket::getsockname(fd: u64, addr: i32, addr_len: i32) -> i32 => "socket/getsockname";

    ported socket::if_nametoindex(name: i32, name_len: i32) -> i32 => "socket/if_nametoindex";

    ported socket::if_indextoname(index: i32) -> u64 => "socket/if_indextoname";

    ported socket::find_ipv6_test_interface() -> i32 => "socket/find_ipv6_test_interface";

    ported socket::udp_client_connect(fd: u64, addr: i32, addr_len: i32) -> i32 => "socket/udp_client_connect";

    helper socket::bind(fd: u64, addr: i32, addr_len: i32) -> i32 => "socket/bind";

    #[cfg(unix)]
    ported socket::recvfrom(fd: u64, buf: i32, offset: i32, len: i32, addr: i32, addr_len: i32) -> i32 => "socket/recvfrom/unix";

    #[cfg(windows)]
    fake socket::recvfrom(fd: u64, buf: i32, offset: i32, len: i32, addr: i32, addr_len: i32) -> i32 => "socket/recvfrom/unix";

    #[cfg(unix)]
    ported socket::sendto(fd: u64, buf: i32, offset: i32, len: i32, addr: i32, addr_len: i32) -> i32 => "socket/sendto/unix";

    #[cfg(windows)]
    fake socket::sendto(fd: u64, buf: i32, offset: i32, len: i32, addr: i32, addr_len: i32) -> i32 => "socket/sendto/unix";

    #[cfg(unix)]
    ported socket::connect(fd: u64, addr: i32, addr_len: i32) -> i32 => "socket/connect/unix";

    #[cfg(windows)]
    fake socket::connect(fd: u64, addr: i32, addr_len: i32) -> i32 => "socket/connect/unix";

    #[cfg(unix)]
    ported socket::getsockerr(fd: u64) -> i32 => "socket/getsockerr/unix";

    #[cfg(windows)]
    fake socket::getsockerr(fd: u64) -> i32 => "socket/getsockerr/unix";

    #[cfg(unix)]
    ported socket::accept(fd: u64, addr: i32, addr_len: i32) -> u64 => "socket/accept/unix";

    #[cfg(windows)]
    fake socket::accept(fd: u64, addr: i32, addr_len: i32) -> u64 => "socket/accept/unix";

    #[cfg(windows)]
    ported socket::connect_io_result(fd: u64, result: u64) -> i32 => "socket/connect/windows";

    #[cfg(not(windows))]
    fake socket::connect_io_result(fd: u64, result: u64) -> i32 => "socket/connect/windows";

    #[cfg(windows)]
    ported socket::setup_connected_socket(fd: u64) -> i32 => "socket/setup_connected_socket/windows";

    #[cfg(not(windows))]
    fake socket::setup_connected_socket(fd: u64) -> i32 => "socket/setup_connected_socket/windows";

    #[cfg(windows)]
    ported socket::accept_io_result(server_fd: u64, conn_fd: u64, result: u64) -> i32 => "socket/accept/windows";

    #[cfg(not(windows))]
    fake socket::accept_io_result(server_fd: u64, conn_fd: u64, result: u64) -> i32 => "socket/accept/windows";

    #[cfg(windows)]
    ported socket::setup_accepted_socket(listen_fd: u64, accept_fd: u64) -> i32 => "socket/setup_accepted_socket/windows";

    #[cfg(not(windows))]
    fake socket::setup_accepted_socket(listen_fd: u64, accept_fd: u64) -> i32 => "socket/setup_accepted_socket/windows";

    #[cfg(windows)]
    ported io::make_file_io_result(
        events: i32,
        len: i32,
        position: i64,
    ) -> u64 => "io/make_file_io_result/windows";

    #[cfg(not(windows))]
    fake io::make_file_io_result(
        events: i32,
        len: i32,
        position: i64,
    ) -> u64 => "io/make_file_io_result/windows";

    #[cfg(windows)]
    ported io::make_socket_io_result(
        events: i32,
        len: i32,
        flags: i32,
    ) -> u64 => "io/make_socket_io_result/windows";

    #[cfg(not(windows))]
    fake io::make_socket_io_result(
        events: i32,
        len: i32,
        flags: i32,
    ) -> u64 => "io/make_socket_io_result/windows";

    #[cfg(windows)]
    ported io::make_socket_with_addr_io_result(
        events: i32,
        len: i32,
        flags: i32,
        addr: i32,
        addr_len: i32,
    ) -> u64 => "io/make_socket_with_addr_io_result/windows";

    #[cfg(not(windows))]
    fake io::make_socket_with_addr_io_result(
        events: i32,
        len: i32,
        flags: i32,
        addr: i32,
        addr_len: i32,
    ) -> u64 => "io/make_socket_with_addr_io_result/windows";

    #[cfg(windows)]
    ported io::make_connect_io_result(addr: i32, addr_len: i32) -> u64 => "io/make_connect_io_result/windows";

    #[cfg(not(windows))]
    fake io::make_connect_io_result(addr: i32, addr_len: i32) -> u64 => "io/make_connect_io_result/windows";

    #[cfg(windows)]
    ported io::make_accept_io_result(addr_len: i32) -> u64 => "io/make_accept_io_result/windows";

    #[cfg(not(windows))]
    fake io::make_accept_io_result(addr_len: i32) -> u64 => "io/make_accept_io_result/windows";

    #[cfg(windows)]
    ported io::get_accept_peer_addr(result: u64, dst: i32, dst_len: i32) -> void => "io/get_accept_peer_addr/windows";

    #[cfg(not(windows))]
    fake io::get_accept_peer_addr(result: u64, dst: i32, dst_len: i32) -> void => "io/get_accept_peer_addr/windows";

    #[cfg(windows)]
    ported io::free_io_result(result: u64) -> void => "io/free_io_result/windows";

    #[cfg(not(windows))]
    fake io::free_io_result(result: u64) -> void => "io/free_io_result/windows";

    #[cfg(windows)]
    ported io::io_result_get_event(result: u64) -> i32 => "io/io_result_get_event/windows";

    #[cfg(not(windows))]
    fake io::io_result_get_event(result: u64) -> i32 => "io/io_result_get_event/windows";

    #[cfg(windows)]
    ported io::cancel_io_result(result: u64, fd: u64) -> i32 => "io/cancel_io_result/windows";

    #[cfg(not(windows))]
    fake io::cancel_io_result(result: u64, fd: u64) -> i32 => "io/cancel_io_result/windows";

    #[cfg(windows)]
    ported io::io_result_get_status(result: u64, fd: u64) -> i32 => "io/io_result_get_status/windows";

    #[cfg(not(windows))]
    fake io::io_result_get_status(result: u64, fd: u64) -> i32 => "io/io_result_get_status/windows";

    #[cfg(windows)]
    helper io::io_result_copy_read(result: u64, dst: i32, offset: i32, len: i32, addr: i32, addr_len: i32) -> void => "io/io_result_copy_read/windows";

    #[cfg(not(windows))]
    fake io::io_result_copy_read(result: u64, dst: i32, offset: i32, len: i32, addr: i32, addr_len: i32) -> void => "io/io_result_copy_read/windows";

    #[cfg(windows)]
    ported io::read_io_result(fd: u64, result: u64) -> i32 => "io/read/windows";

    #[cfg(not(windows))]
    fake io::read_io_result(fd: u64, result: u64) -> i32 => "io/read/windows";

    #[cfg(windows)]
    ported io::write_io_result(fd: u64, result: u64, src: i32, offset: i32, len: i32) -> i32 => "io/write/windows";

    #[cfg(not(windows))]
    fake io::write_io_result(fd: u64, result: u64, src: i32, offset: i32, len: i32) -> i32 => "io/write/windows";

    #[cfg(windows)]
    ported io::errno_is_read_eof(errno: i32) -> i32 => "io/errno_is_read_EOF/windows";

    #[cfg(not(windows))]
    fake io::errno_is_read_eof(errno: i32) -> i32 => "io/errno_is_read_EOF/windows";

    helper c_buffer::is_null(ptr: u64) -> i32 => "c_buffer/is_null";

    ported c_buffer::blit_to_c(dst: u64, dst_offset: i32, src: i32, src_offset: i32, len: i32) -> void => "c_buffer/blit_to_c";

    ported c_buffer::blit_from_c(src: u64, src_offset: i32, dst: i32, dst_offset: i32, len: i32) -> void => "c_buffer/blit_from_c";

    ported c_buffer::c_buffer_get(buf: u64, index: i32) -> i32 => "c_buffer/c_buffer_get";

    ported c_buffer::strlen(buf: u64) -> i32 => "c_buffer/strlen";

    ported c_buffer::new(size: i32) -> u64 => "c_buffer/new";

    helper c_buffer::free(ptr: u64) -> void => "c_buffer/free";

    // fs/stub.c and fs/dir.c.
    ported fs::errno_is_lock_violation(errno: i32) -> i32 => "fs/errno_is_lock_violation";

    ported fs::try_lock_file(fd: u64, exclusive: i32) -> i32 => "fs/try_lock_file";

    ported fs::unlock_file(fd: u64) -> i32 => "fs/unlock_file";

    // Returns the UTF-16 code-unit length that the guest must allocate for
    // `fs/get_tmp_path`.
    helper fs::get_tmp_path_len() -> i32 => "fs/get_tmp_path_len";

    // Writes the native temporary directory as UTF-16 code units into a
    // guest-allocated MoonBit String.
    ported fs::get_tmp_path(ptr: i32, len: i32) -> i32 => "fs/get_tmp_path";

    helper fs::get_tmp_path_buffer() -> u64 => "fs/get_tmp_path_buffer";

    ported fs::dir_buffer_min_size() -> i32 => "fs/dir_buffer_min_size";

    ported fs::dir_entry_length(buf: u64, offset: i32) -> i32 => "fs/dir_entry_length";

    ported fs::dir_entry_name_len(buf: u64, offset: i32) -> i32 => "fs/dir_entry_get_name_len";

    ported fs::dir_entry_name_offset(buf: u64, offset: i32) -> i32 => "fs/dir_entry_get_name_offset";

    ported fs::dir_entry_is_dir(buf: u64, offset: i32) -> i32 => "fs/dir_entry_is_dir";

    ported fs::dir_entry_is_hidden(buf: u64, offset: i32) -> i32 => "fs/dir_entry_is_hidden";

    ported fs::dir_entry_file_id(buf: u64, offset: i32) -> u64 => "fs/dir_entry_get_file_id";

    // thread_pool.c FS jobs. Path-taking jobs use the Guest String Path ABI:
    // MoonBit String pointer plus UTF-16 code-unit length.
    ported thread_pool::make_open_job(
        path_ptr: i32,
        path_len: i32,
        access: i32,
        create_mode: i32,
        append: i32,
        sync: i32,
        mode: i32,
    ) -> u64 => "thread_pool/make_open_job";

    ported thread_pool::open_job_get_fd(job: u64) -> u64 => "thread_pool/open_job_get_fd";

    ported thread_pool::open_job_get_kind(job: u64) -> i32 => "thread_pool/open_job_get_kind";

    ported thread_pool::open_job_get_dev_id(job: u64) -> u64 => "thread_pool/open_job_get_dev_id";

    ported thread_pool::open_job_get_file_id(job: u64) -> u64 => "thread_pool/open_job_get_file_id";

    ported thread_pool::make_read_job(fd: u64, len: i32, position: i64) -> u64 => "thread_pool/make_read_job";

    ported thread_pool::make_write_job(fd: u64, ptr: i32, offset: i32, len: i32, position: i64) -> u64 => "thread_pool/make_write_job";

    helper thread_pool::get_read_result(job: u64, dst: i32, offset: i32, len: i32) -> void => "thread_pool/get_read_result";

    ported thread_pool::make_file_kind_by_path_job(
        parent: u64,
        path_ptr: i32,
        path_len: i32,
        follow_symlink: i32,
    ) -> u64 => "thread_pool/make_file_kind_by_path_job";

    ported thread_pool::make_file_size_job(fd: u64) -> u64 => "thread_pool/make_file_size_job";

    ported thread_pool::get_file_size_result(job: u64) -> i64 => "thread_pool/get_file_size_result";

    ported thread_pool::make_file_time_job(fd: u64) -> u64 => "thread_pool/make_file_time_job";

    ported thread_pool::make_file_time_by_path_job(
        path_ptr: i32,
        path_len: i32,
        follow_symlink: i32,
    ) -> u64 => "thread_pool/make_file_time_by_path_job";

    helper thread_pool::get_file_time_result(job: u64, out: i32) -> void => "thread_pool/get_file_time_result";

    ported thread_pool::make_access_job(path_ptr: i32, path_len: i32, access: i32) -> u64 => "thread_pool/make_access_job";

    ported thread_pool::make_chmod_job(path_ptr: i32, path_len: i32, mode: i32) -> u64 => "thread_pool/make_chmod_job";

    ported thread_pool::make_fsync_job(fd: u64, only_data: i32) -> u64 => "thread_pool/make_fsync_job";

    ported thread_pool::make_flock_job(fd: u64, exclusive: i32) -> u64 => "thread_pool/make_flock_job";

    ported thread_pool::make_remove_job(path_ptr: i32, path_len: i32) -> u64 => "thread_pool/make_remove_job";

    ported thread_pool::make_rename_job(
        old_path_ptr: i32,
        old_path_len: i32,
        new_path_ptr: i32,
        new_path_len: i32,
        replace: i32,
    ) -> u64 => "thread_pool/make_rename_job";

    ported thread_pool::make_symlink_job(
        target_ptr: i32,
        target_len: i32,
        path_ptr: i32,
        path_len: i32,
        force_symlink: i32,
    ) -> u64 => "thread_pool/make_symlink_job";

    ported thread_pool::make_mkdir_job(path_ptr: i32, path_len: i32, mode: i32) -> u64 => "thread_pool/make_mkdir_job";

    ported thread_pool::make_rmdir_job(path_ptr: i32, path_len: i32) -> u64 => "thread_pool/make_rmdir_job";

    ported thread_pool::make_readdir_job(dir: u64, buf: u64, len: i32, restart: i32) -> u64 => "thread_pool/make_readdir_job";

    ported thread_pool::make_bind_job(socket: u64, addr: i32, addr_len: i32) -> u64 => "thread_pool/make_bind_job";

    ported thread_pool::make_getaddrinfo_job(host: i32, host_len: i32) -> u64 => "thread_pool/make_getaddrinfo_job";

    helper thread_pool::get_getaddrinfo_result(job: u64) -> u64 => "thread_pool/get_getaddrinfo_result";
}

#[cfg(test)]
fn async_api_ported_imports() -> Vec<PortedImport> {
    let mut imports = Vec::new();
    imports.extend_from_slice(event_loop::PORTED_IMPORTS);
    imports.extend_from_slice(event_bus::PORTED_IMPORTS);
    imports.extend_from_slice(thread_pool::PORTED_IMPORTS);
    imports.extend_from_slice(os_error::PORTED_IMPORTS);
    imports.extend_from_slice(fs::PORTED_IMPORTS);
    imports.extend_from_slice(fd_util::PORTED_IMPORTS);
    imports.extend_from_slice(io::PORTED_IMPORTS);
    imports.extend_from_slice(env_util::PORTED_IMPORTS);
    imports.extend_from_slice(c_buffer::PORTED_IMPORTS);
    imports.extend_from_slice(socket::PORTED_IMPORTS);
    imports
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
        }
    }

    fn native_symbol(import: &AsyncImport) -> String {
        let segments = import.wasm_symbol.split('/').collect::<Vec<_>>();
        let leaf = match segments.as_slice() {
            [.., symbol, "linux" | "macos" | "unix" | "windows"] => symbol,
            [.., symbol] => symbol,
            [] => import.callback_symbol,
        };
        format!("{NATIVE_ASYNC_PREFIX}{leaf}")
    }

    fn native_symbol_for(import: &AsyncImport, ported_import: &PortedImport) -> String {
        ported_import
            .native_symbol
            .map(str::to_string)
            .unwrap_or_else(|| native_symbol(import))
    }

    fn module_leaf(module_path: &str) -> Option<&str> {
        module_path.rsplit("::").next()
    }

    fn registered_import_for_ported(ported_import: &PortedImport) -> Option<&'static AsyncImport> {
        ASYNC_IMPORTS.iter().find(|import| {
            module_leaf(ported_import.rust_module) == Some(import.callback_module)
                && ported_import.rust_symbol == import.callback_symbol
        })
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
    fn runtime_exit_is_part_of_moonbit_async() {
        assert!(
            ASYNC_IMPORTS.iter().any(|import| {
                import.kind == AsyncImportKind::Helper && import.wasm_symbol == "runtime/exit"
            }),
            "async wasm integration must not depend on older runtime namespaces for exit"
        );
    }

    #[test]
    fn event_bus_imports_are_the_event_loop_boundary() {
        let imports = ASYNC_IMPORTS
            .iter()
            .map(|import| import.wasm_symbol)
            .collect::<BTreeSet<_>>();
        assert!(
            !imports.contains("runtime/wait_for_event"),
            "async wasm event loop must not use runtime/wait_for_event"
        );
        for wasm_symbol in [
            "event_bus/create",
            "event_bus/destroy",
            "event_bus/register",
            "event_bus/wait",
            "event_bus/get_event",
            "event_bus/event_fd",
            "event_bus/event_events/unix",
            "event_bus/event_io_result/windows",
            "event_bus/event_bytes_transferred/windows",
        ] {
            assert!(
                imports.contains(wasm_symbol),
                "missing async wasm event bus import {wasm_symbol}"
            );
        }
    }

    #[test]
    fn wasm_import_names_are_namespaced() {
        for import in ASYNC_IMPORTS {
            let Some((_namespace, _)) = import.wasm_symbol.split_once('/') else {
                panic!("async import {} must be namespaced", import.wasm_symbol);
            };
            for segment in import.wasm_symbol.split('/') {
                assert!(
                    !segment.is_empty(),
                    "empty path segment for {}",
                    import.wasm_symbol
                );
            }
            let leaf = import
                .wasm_symbol
                .rsplit('/')
                .next()
                .expect("split must produce at least one segment");
            assert!(!leaf.starts_with("async_"));
        }
    }

    #[test]
    fn declared_sources_exist_and_contain_native_symbols() {
        for ported_import in async_api_ported_imports() {
            let registered_import = registered_import_for_ported(&ported_import)
                .expect("ported import must have a registered wasm import");
            let native_symbol = native_symbol_for(registered_import, &ported_import);
            for source in ported_import.sources {
                let source_path = source_path(*source);
                let contents = fs::read_to_string(&source_path)
                    .unwrap_or_else(|error| panic!("failed to read {:?}: {error}", source_path));
                assert!(
                    contents.contains(&native_symbol),
                    "{:?} does not contain native symbol {} for wasm import {}",
                    source_path,
                    native_symbol,
                    registered_import.wasm_symbol
                );
            }
        }
    }

    #[test]
    fn ported_provenance_entries_are_registered_imports() {
        for ported_import in async_api_ported_imports() {
            assert!(
                registered_import_for_ported(&ported_import).is_some(),
                "ported provenance for {:?}::{:?} has no registered import",
                ported_import.rust_module,
                ported_import.rust_symbol
            );
        }
    }

    #[test]
    fn fake_imports_do_not_have_active_provenance() {
        let api_ported_imports = async_api_ported_imports();
        for import in ASYNC_IMPORTS {
            if import.kind != AsyncImportKind::Fake {
                continue;
            }
            assert!(
                api_ported_imports.iter().all(|ported_import| {
                    module_leaf(ported_import.rust_module) != Some(import.callback_module)
                        || ported_import.rust_symbol != import.callback_symbol
                }),
                "fake import {} has active ported provenance",
                import.wasm_symbol
            );
        }
    }

    #[test]
    fn ported_imports_have_ported_implementations() {
        let api_ported_imports = async_api_ported_imports();
        let ported_symbols = crate::async_sys::ported_symbols();

        for import in ASYNC_IMPORTS {
            if import.kind != AsyncImportKind::Ported {
                continue;
            }

            let ported_import = api_ported_imports
                .iter()
                .find(|ported_import| {
                    module_leaf(ported_import.rust_module) == Some(import.callback_module)
                        && ported_import.rust_symbol == import.callback_symbol
                })
                .unwrap_or_else(|| {
                    panic!(
                        "async import {} has no ported provenance",
                        import.wasm_symbol
                    )
                });
            let native_symbol = native_symbol_for(import, ported_import);
            assert!(
                ported_import.sources.iter().any(|source| {
                    source.root == SourceRoot::MoonbitAsync
                        && ported_symbols.iter().any(|ported| {
                            ported.native_symbol == native_symbol && ported.source == source.path
                        })
                }),
                "async import {} has no Rust port origin",
                import.wasm_symbol
            );
        }
    }

    #[test]
    fn ported_implementations_are_registered_imports() {
        let api_ported_imports = async_api_ported_imports();
        for ported in crate::async_sys::ported_symbols() {
            assert!(
                api_ported_imports.iter().any(|ported_import| {
                    let Some(registered_import) = registered_import_for_ported(ported_import)
                    else {
                        return false;
                    };
                    let native_symbol = native_symbol_for(registered_import, ported_import);
                    ASYNC_IMPORTS
                        .iter()
                        .any(|import| import.wasm_symbol == registered_import.wasm_symbol)
                        && ported_import.sources.iter().any(|source| {
                            native_symbol == ported.native_symbol
                                && source.root == SourceRoot::MoonbitAsync
                                && source.path == ported.source
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
