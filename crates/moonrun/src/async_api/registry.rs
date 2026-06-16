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

use super::{event_loop, os_error, runtime, thread_pool, time, unsupported};

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
        ) {
            $(
                register_func_impl(obj, scope, $wasm_symbol, $callback);
            )*
        }
    };
}

fn register_func_impl<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
) {
    obj.set_func(scope, name, callback);
}

// Complete `moonbit_v0` async ABI surface registered by this split PR.
//
// Entry shape:
//   kind callback maps to "namespace/wasm_symbol",
//   native = Some("moonbitlang_async_native_symbol") | None,
//   sources = [moonbit_async:"path/in/async", moonrun:"path/in/moonrun"];
//
// Kind legend:
// - native: a supported import whose semantics are expected to follow the
//   corresponding native C stub.
// - support: wasm-only host glue or a supported import with wasm-specific
//   behavior. A native symbol may still be listed for provenance.
// - unsupported: registered for link compatibility and one-to-one C-stub
//   inventory only; it fails loudly if PR1 accidentally reaches that surface.
declare_async_imports! {
    support runtime::exit => "runtime/exit",
    native = None,
    sources = [
        moonbit_async:"src/integration.wasm.mbt",
        moonrun:"crates/moonrun/src/async_api/runtime.rs",
    ];

    // Wasm-only timer wait glue. This is not a native C-stub mapping; native
    // `moonbitlang_async_poll_wait` is listed below as unsupported `poll/poll_wait`.
    support runtime::wait_for_event => "runtime/wait_for_event",
    native = None,
    sources = [
        moonbit_async:"src/internal/event_loop/event_loop.wasm.mbt",
        moonrun:"crates/moonrun/src/async_api/runtime.rs",
    ];

    native event_loop::get_platform => "runtime/get_platform",
    native = Some("moonbitlang_async_get_platform"),
    sources = [
        moonbit_async:"src/internal/event_loop/thread_pool.c",
        moonbit_async:"src/internal/event_loop/event_loop.wasm.mbt",
        moonrun:"crates/moonrun/src/async_api/event_loop.rs",
    ];

    support time::get_ms_since_epoch => "time/get_ms_since_epoch",
    native = None,
    sources = [
        moonbit_async:"src/internal/event_loop/event_loop.wasm.mbt",
        moonrun:"crates/moonrun/src/async_api/time.rs",
    ];

    native os_error::get_errno => "os_error/get_errno",
    native = Some("moonbitlang_async_get_errno"),
    sources = [
        moonbit_async:"src/os_error/stub.c",
        moonbit_async:"src/os_error/error.wasm.mbt",
        moonrun:"crates/moonrun/src/async_api/os_error.rs",
    ];

    support event_loop::errno_is_cancelled => "thread_pool/errno_is_cancelled",
    native = Some("moonbitlang_async_errno_is_cancelled"),
    sources = [
        moonbit_async:"src/internal/event_loop/thread_pool.c",
        moonbit_async:"src/internal/event_loop/event_loop.wasm.mbt",
        moonrun:"crates/moonrun/src/async_api/event_loop.rs",
    ];

    support thread_pool::fetch_completion => "thread_pool/fetch_completion",
    native = Some("moonbitlang_async_fetch_completion"),
    sources = [
        moonbit_async:"src/internal/event_loop/thread_pool.c",
        moonbit_async:"src/internal/event_loop/thread_pool.wasm.mbt",
        moonrun:"crates/moonrun/src/async_api/thread_pool.rs",
    ];

    // Native poller C ABI. PR1 does not expose the native fd/event-list poller,
    // but the exact C stubs are still registered as unsupported ABI inventory.
    unsupported unsupported::fail => "poll/poll_create",
    native = Some("moonbitlang_async_poll_create"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
        moonbit_async:"src/internal/event_loop/iocp.c",
    ];

    unsupported unsupported::fail => "poll/poll_destroy",
    native = Some("moonbitlang_async_poll_destroy"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
        moonbit_async:"src/internal/event_loop/iocp.c",
    ];

    unsupported unsupported::fail => "poll/poll_register",
    native = Some("moonbitlang_async_poll_register"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
        moonbit_async:"src/internal/event_loop/iocp.c",
    ];

    unsupported unsupported::fail => "poll/support_wait_pid_via_poll",
    native = Some("moonbitlang_async_support_wait_pid_via_poll"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
    ];

    unsupported unsupported::fail => "poll/poll_register_pid",
    native = Some("moonbitlang_async_poll_register_pid"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
    ];

    unsupported unsupported::fail => "poll/poll_remove",
    native = Some("moonbitlang_async_poll_remove"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
    ];

    unsupported unsupported::fail => "poll/poll_remove_pid",
    native = Some("moonbitlang_async_poll_remove_pid"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
    ];

    unsupported unsupported::fail => "poll/poll_wait",
    native = Some("moonbitlang_async_poll_wait"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
        moonbit_async:"src/internal/event_loop/iocp.c",
    ];

    unsupported unsupported::fail => "poll/event_list_get",
    native = Some("moonbitlang_async_event_list_get"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
        moonbit_async:"src/internal/event_loop/iocp.c",
    ];

    unsupported unsupported::fail => "poll/event_get_fd",
    native = Some("moonbitlang_async_event_get_fd"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
        moonbit_async:"src/internal/event_loop/iocp.c",
    ];

    unsupported unsupported::fail => "poll/event_get_events",
    native = Some("moonbitlang_async_event_get_events"),
    sources = [
        moonbit_async:"src/internal/event_loop/epoll.c",
        moonbit_async:"src/internal/event_loop/kqueue.c",
    ];

    unsupported unsupported::fail => "poll/event_get_io_result",
    native = Some("moonbitlang_async_event_get_io_result"),
    sources = [moonbit_async:"src/internal/event_loop/iocp.c"];

    unsupported unsupported::fail => "poll/event_get_bytes_transferred",
    native = Some("moonbitlang_async_event_get_bytes_transferred"),
    sources = [moonbit_async:"src/internal/event_loop/iocp.c"];

    unsupported unsupported::fail => "thread_pool/spawn_worker",
    native = Some("moonbitlang_async_spawn_worker"),
    sources = [
        moonbit_async:"src/internal/event_loop/thread_pool.c",
        moonbit_async:"src/internal/event_loop/thread_pool.wasm.mbt",
        moonrun:"crates/moonrun/src/async_api/unsupported.rs",
    ];

    unsupported unsupported::fail => "thread_pool/free_worker",
    native = Some("moonbitlang_async_free_worker"),
    sources = [
        moonbit_async:"src/internal/event_loop/thread_pool.c",
        moonbit_async:"src/internal/event_loop/thread_pool.wasm.mbt",
        moonrun:"crates/moonrun/src/async_api/unsupported.rs",
    ];

    unsupported unsupported::fail => "thread_pool/wake_worker",
    native = Some("moonbitlang_async_wake_worker"),
    sources = [
        moonbit_async:"src/internal/event_loop/thread_pool.c",
        moonbit_async:"src/internal/event_loop/thread_pool.wasm.mbt",
        moonrun:"crates/moonrun/src/async_api/unsupported.rs",
    ];

    unsupported unsupported::fail => "thread_pool/worker_enter_idle",
    native = Some("moonbitlang_async_worker_enter_idle"),
    sources = [
        moonbit_async:"src/internal/event_loop/thread_pool.c",
        moonbit_async:"src/internal/event_loop/thread_pool.wasm.mbt",
        moonrun:"crates/moonrun/src/async_api/unsupported.rs",
    ];

    unsupported unsupported::fail => "thread_pool/cancel_worker",
    native = Some("moonbitlang_async_cancel_worker"),
    sources = [
        moonbit_async:"src/internal/event_loop/thread_pool.c",
        moonbit_async:"src/internal/event_loop/thread_pool.wasm.mbt",
        moonrun:"crates/moonrun/src/async_api/unsupported.rs",
    ];

    unsupported unsupported::fail => "thread_pool/free_job",
    native = Some("moonbitlang_async_free_job"),
    sources = [
        moonbit_async:"src/internal/event_loop/thread_pool.c",
        moonbit_async:"src/internal/event_loop/thread_pool.wasm.mbt",
        moonrun:"crates/moonrun/src/async_api/unsupported.rs",
    ];

    unsupported unsupported::fail => "thread_pool/run_job",
    native = None,
    sources = [
        moonbit_async:"src/internal/event_loop/thread_pool.wasm.mbt",
        moonrun:"crates/moonrun/src/async_api/unsupported.rs",
    ];

    unsupported unsupported::fail => "thread_pool/job_get_ret",
    native = Some("moonbitlang_async_job_get_ret"),
    sources = [
        moonbit_async:"src/internal/event_loop/thread_pool.c",
        moonbit_async:"src/internal/event_loop/thread_pool.wasm.mbt",
        moonrun:"crates/moonrun/src/async_api/unsupported.rs",
    ];

    unsupported unsupported::fail => "thread_pool/job_get_err",
    native = Some("moonbitlang_async_job_get_err"),
    sources = [
        moonbit_async:"src/internal/event_loop/thread_pool.c",
        moonbit_async:"src/internal/event_loop/thread_pool.wasm.mbt",
        moonrun:"crates/moonrun/src/async_api/unsupported.rs",
    ];
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    use super::*;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .unwrap()
            .to_path_buf()
    }

    fn source_path(source: SourceLocation) -> PathBuf {
        match source.root {
            SourceRoot::MoonbitAsync => repo_root()
                .join("third_party/moonbitlang_async")
                .join(source.path),
            SourceRoot::Moonrun => repo_root().join(source.path),
        }
    }

    fn collect_moonbit_files(dir: &Path, files: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect_moonbit_files(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "mbt") {
                files.push(path);
            }
        }
    }

    fn moonbit_v0_imports() -> HashSet<String> {
        let marker = "\"moonbit_v0\" \"";
        let mut files = Vec::new();
        collect_moonbit_files(
            &repo_root().join("third_party/moonbitlang_async/src"),
            &mut files,
        );

        let mut imports = HashSet::new();
        for file in files {
            let contents = std::fs::read_to_string(file).unwrap();
            for line in contents.lines() {
                let mut rest = line;
                while let Some(start) = rest.find(marker) {
                    let symbol_start = start + marker.len();
                    rest = &rest[symbol_start..];
                    if let Some(end) = rest.find('"') {
                        imports.insert(rest[..end].to_owned());
                        rest = &rest[end..];
                    } else {
                        break;
                    }
                }
            }
        }
        imports
    }

    #[test]
    fn import_names_are_unique() {
        let mut names = HashSet::new();
        for import in ASYNC_IMPORTS {
            assert!(
                names.insert(import.wasm_symbol),
                "duplicate async import {}",
                import.wasm_symbol
            );
        }
    }

    #[test]
    fn registry_covers_async_wasm_imports() {
        let declared = ASYNC_IMPORTS
            .iter()
            .map(|import| import.wasm_symbol)
            .collect::<HashSet<_>>();
        let actual = moonbit_v0_imports();
        for import in actual {
            assert!(
                declared.contains(import.as_str()),
                "missing moonbit_v0 async import registration for {import}"
            );
        }
    }

    #[test]
    fn source_locations_exist() {
        for import in ASYNC_IMPORTS {
            for source in import.sources {
                let path = source_path(*source);
                assert!(
                    path.exists(),
                    "source for {} does not exist: {}",
                    import.wasm_symbol,
                    path.display()
                );
            }
        }
    }

    #[test]
    fn native_symbols_are_traceable_to_async_sources() {
        for import in ASYNC_IMPORTS {
            let Some(native_symbol) = import.native_symbol else {
                continue;
            };
            assert!(
                native_symbol.starts_with(NATIVE_ASYNC_PREFIX),
                "native symbol for {} should use {NATIVE_ASYNC_PREFIX}: {native_symbol}",
                import.wasm_symbol
            );

            let found = import
                .sources
                .iter()
                .filter(|source| source.root == SourceRoot::MoonbitAsync)
                .map(|source| std::fs::read_to_string(source_path(*source)).unwrap())
                .any(|contents| contents.contains(native_symbol));
            assert!(
                found,
                "native symbol {native_symbol} for {} is not present in its async sources",
                import.wasm_symbol
            );
        }
    }

    #[test]
    fn pr1_surface_stays_event_loop_timer_only() {
        for import in ASYNC_IMPORTS {
            assert!(
                !import.wasm_symbol.starts_with("fs/")
                    && !import.wasm_symbol.starts_with("process/")
                    && !import.wasm_symbol.starts_with("socket/")
                    && !import.wasm_symbol.starts_with("tls/")
                    && !import.wasm_symbol.starts_with("fd_util/")
                    && !import.wasm_symbol.starts_with("c_buffer/"),
                "PR1 should not register broader async host import {}",
                import.wasm_symbol
            );
        }
    }

    #[test]
    fn native_poll_imports_are_explicit_unsupported_entries() {
        let wait_for_event = ASYNC_IMPORTS
            .iter()
            .find(|import| import.wasm_symbol == "runtime/wait_for_event")
            .expect("runtime/wait_for_event should be registered");
        assert_eq!(wait_for_event.kind, AsyncImportKind::WasmSupport);
        assert_eq!(wait_for_event.native_symbol, None);

        for (wasm_symbol, native_symbol) in [
            ("poll/poll_create", "moonbitlang_async_poll_create"),
            ("poll/poll_destroy", "moonbitlang_async_poll_destroy"),
            ("poll/poll_register", "moonbitlang_async_poll_register"),
            (
                "poll/support_wait_pid_via_poll",
                "moonbitlang_async_support_wait_pid_via_poll",
            ),
            (
                "poll/poll_register_pid",
                "moonbitlang_async_poll_register_pid",
            ),
            ("poll/poll_remove", "moonbitlang_async_poll_remove"),
            ("poll/poll_remove_pid", "moonbitlang_async_poll_remove_pid"),
            ("poll/poll_wait", "moonbitlang_async_poll_wait"),
            ("poll/event_list_get", "moonbitlang_async_event_list_get"),
            ("poll/event_get_fd", "moonbitlang_async_event_get_fd"),
            (
                "poll/event_get_events",
                "moonbitlang_async_event_get_events",
            ),
            (
                "poll/event_get_io_result",
                "moonbitlang_async_event_get_io_result",
            ),
            (
                "poll/event_get_bytes_transferred",
                "moonbitlang_async_event_get_bytes_transferred",
            ),
        ] {
            let import = ASYNC_IMPORTS
                .iter()
                .find(|import| import.wasm_symbol == wasm_symbol)
                .unwrap_or_else(|| panic!("{wasm_symbol} should be registered"));
            assert_eq!(import.kind, AsyncImportKind::UnsupportedMvp);
            assert_eq!(import.native_symbol, Some(native_symbol));
        }
    }

    #[test]
    fn worker_job_imports_are_link_stubs() {
        let unsupported = ASYNC_IMPORTS
            .iter()
            .filter(|import| import.kind == AsyncImportKind::UnsupportedMvp)
            .map(|import| import.wasm_symbol)
            .collect::<HashSet<_>>();
        for symbol in [
            "thread_pool/spawn_worker",
            "thread_pool/free_worker",
            "thread_pool/wake_worker",
            "thread_pool/worker_enter_idle",
            "thread_pool/cancel_worker",
            "thread_pool/free_job",
            "thread_pool/run_job",
            "thread_pool/job_get_ret",
            "thread_pool/job_get_err",
        ] {
            assert!(
                unsupported.contains(symbol),
                "{symbol} should be unsupported in PR1"
            );
        }
    }
}
