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

//! V8-free async host operations ported from `moonbitlang/async` native stubs.
//!
//! The provenance macros link host implementations to their upstream source
//! paths and symbols independently of the Wasm ABI adapters. Implementations
//! also live in the host domains; no matching Wasm import is required.

pub(crate) mod internal;
pub(crate) mod os_error;
pub(crate) mod signal;

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SourceRoot {
    MoonbitAsync,
    // Native primitives shared with core/async and shipped with the toolchain.
    MoonbitRuntime,
}

#[cfg(test)]
impl SourceRoot {
    pub(crate) fn path(self, source: &str) -> std::path::PathBuf {
        match self {
            Self::MoonbitAsync => std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../third_party/moonbitlang_async")
                .join(source),
            Self::MoonbitRuntime => moonutil::toolchain::toolchain_root()
                .join("lib/runtime")
                .join(source),
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PortedSymbol {
    pub(crate) rust_module: &'static str,
    pub(crate) rust_symbol: &'static str,
    pub(crate) native_symbol: &'static str,
    pub(crate) root: SourceRoot,
    pub(crate) source: &'static str,
}

#[cfg(test)]
impl PortedSymbol {
    pub(crate) fn assert_source_exists(&self) {
        let source_path = self.root.path(self.source);
        let contents = std::fs::read_to_string(&source_path).unwrap_or_else(|error| {
            panic!(
                "failed to read upstream source {source_path:?} for {}::{}: {error}",
                self.rust_module, self.rust_symbol,
            )
        });
        let symbol =
            regex::Regex::new(&format!(r"(?-u:\b{}\b)", regex::escape(self.native_symbol)))
                .expect("escaped upstream symbol is a valid regex");
        assert!(
            symbol.is_match(&contents),
            "upstream source {source_path:?} does not contain symbol {} for {}::{}",
            self.native_symbol,
            self.rust_module,
            self.rust_symbol,
        );
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CompatSymbol {
    pub(crate) rust_module: &'static str,
    pub(crate) rust_symbol: &'static str,
    pub(crate) native_symbol: &'static str,
    pub(crate) historical_source: &'static str,
    pub(crate) upstream_pr: u32,
    pub(crate) replacement: &'static str,
}

// An implementation can combine several upstream functions, or have no Wasm
// import at all. Emit provenance only for tests; leave its executable body intact.
macro_rules! ported_fns {
    (@root source) => { crate::async_sys::SourceRoot::MoonbitAsync };
    (@root runtime) => { crate::async_sys::SourceRoot::MoonbitRuntime };
    (@collect [$prefix:expr] [$($entries:tt)*] [$($compat_entries:tt)*] [$($out:tt)*]) => {
        #[cfg(test)]
        pub(crate) const PORTED_SYMBOLS: &[crate::async_sys::PortedSymbol] = &[
            $($entries)*
        ];

        #[cfg(test)]
        #[allow(dead_code)]
        pub(crate) const COMPAT_SYMBOLS: &[crate::async_sys::CompatSymbol] = &[
            $($compat_entries)*
        ];

        $($out)*
    };
    (
        @collect [$prefix:expr] [$($entries:tt)*] [$($compat_entries:tt)*] [$($out:tt)*]
        #[ported($source_root:ident = $source:literal, original = $original:literal)]
        $($rest:tt)*
    ) => {
        ported_fns!(
            @collect [$prefix] [$($entries)*] [$($compat_entries)*] [$($out)*]
            #[ported($source_root = $source, original = [$original])]
            $($rest)*
        );
    };
    (
        @collect [$prefix:expr] [$($entries:tt)*] [$($compat_entries:tt)*] [$($out:tt)*]
        #[ported($source_root:ident = $source:literal, original = [$($original:literal),+ $(,)?])]
        #[cfg($cfg:meta)]
        $(#[$meta:meta])*
        $vis:vis $first:ident $($rest:tt)*
    ) => {
        ported_fns!(
            @function [$prefix] [$($entries)*] [$($compat_entries)*] [$($out)*]
            [$source_root = $source, [$($original),+]] [$cfg]
            [$(#[$meta])*] [$vis] [] $first $($rest)*
        );
    };
    (
        @collect [$prefix:expr] [$($entries:tt)*] [$($compat_entries:tt)*] [$($out:tt)*]
        #[ported($source_root:ident = $source:literal, original = [$($original:literal),+ $(,)?])]
        $(#[$meta:meta])*
        $vis:vis $first:ident $($rest:tt)*
    ) => {
        ported_fns!(
            @function [$prefix] [$($entries)*] [$($compat_entries)*] [$($out)*]
            [$source_root = $source, [$($original),+]] [all()]
            [$(#[$meta])*] [$vis] [] $first $($rest)*
        );
    };
    (
        @function [$prefix:expr] [$($entries:tt)*] [$($compat_entries:tt)*] [$($out:tt)*]
        [$source_root:ident = $source:literal, [$($original:literal),+]]
        [$cfg:meta] [$($attrs:tt)*] [$vis:vis] [] unsafe $($rest:tt)*
    ) => {
        ported_fns!(
            @function [$prefix] [$($entries)*] [$($compat_entries)*] [$($out)*]
            [$source_root = $source, [$($original),+]] [$cfg]
            [$($attrs)*] [$vis] [unsafe] $($rest)*
        );
    };
    (
        @function [$prefix:expr] [$($entries:tt)*] [$($compat_entries:tt)*] [$($out:tt)*]
        [$source_root:ident = $source:literal, [$($original:literal),+]]
        [$cfg:meta] [$($attrs:tt)*] [$vis:vis] [$($safety:tt)*]
        fn $name:ident($($args:tt)*) $(-> $ret:ty)? $body:block
        $($rest:tt)*
    ) => {
        ported_fns!(
            @collect [$prefix] [
                $($entries)*
                $(
                    #[cfg($cfg)]
                    crate::async_sys::PortedSymbol {
                        rust_module: module_path!(),
                        rust_symbol: concat!($prefix, stringify!($name)),
                        native_symbol: $original,
                        root: ported_fns!(@root $source_root),
                        source: $source,
                    },
                )+
            ] [$($compat_entries)*] [
                $($out)*
                #[cfg($cfg)]
                $($attrs)*
                $vis $($safety)* fn $name($($args)*) $(-> $ret)? $body
            ]
            $($rest)*
        );
    };
    (
        @collect [$prefix:expr] [$($entries:tt)*] [$($compat_entries:tt)*] [$($out:tt)*]
        #[compat(
            source = $source:literal,
            original = $original:literal,
            upstream_pr = $upstream_pr:literal,
            replacement = $replacement:literal
        )]
        $(#[$meta:meta])*
        $vis:vis fn $name:ident($($args:tt)*) $(-> $ret:ty)? $body:block
        $($rest:tt)*
    ) => {
        ported_fns!(
            @collect [$prefix] [$($entries)*] [
                $($compat_entries)*
                crate::async_sys::CompatSymbol {
                    rust_module: module_path!(),
                    rust_symbol: concat!($prefix, stringify!($name)),
                    native_symbol: $original,
                    historical_source: $source,
                    upstream_pr: $upstream_pr,
                    replacement: $replacement,
                },
            ] [
                $($out)*
                $(#[$meta])*
                $vis fn $name($($args)*) $(-> $ret)? $body
            ]
            $($rest)*
        );
    };
    (@collect [$prefix:expr] [$($entries:tt)*] [$($compat_entries:tt)*] [$($out:tt)*] $item:item $($rest:tt)*) => {
        ported_fns!(@collect [$prefix] [$($entries)*] [$($compat_entries)*] [$($out)* $item] $($rest)*);
    };
    (impl $self_ty:ty { $($items:tt)* }) => {
        impl $self_ty {
            ported_fns!(@collect [concat!(stringify!($self_ty), "::")] [] [] [] $($items)*);
        }
    };
    ($($items:tt)*) => {
        ported_fns!(@collect [""] [] [] [] $($items)*);
    };
}

pub(crate) use ported_fns;

#[cfg(test)]
pub(crate) fn ported_symbols() -> Vec<PortedSymbol> {
    let mut symbols = Vec::new();
    symbols.extend_from_slice(crate::async_host::buffers::Buffers::PORTED_SYMBOLS);
    symbols.extend_from_slice(internal::env_util::stub::PORTED_SYMBOLS);
    symbols.extend_from_slice(crate::filesystem::PIPE_PORTED_SYMBOLS);
    symbols.extend(internal::event_loop::io::ported_symbols());
    symbols.extend_from_slice(crate::filesystem::IO_PORTED_SYMBOLS);
    symbols.extend_from_slice(crate::filesystem::LOCK_PORTED_SYMBOLS);
    symbols.extend(crate::poll::ported_symbols());
    symbols.extend(crate::thread_pool::ported_symbols());
    symbols.extend_from_slice(crate::filesystem::directory_entries::PORTED_SYMBOLS);
    symbols.extend_from_slice(crate::filesystem::TEMP_DIR_PORTED_SYMBOLS);
    symbols.extend_from_slice(crate::filesystem::watch::PORTED_SYMBOLS);
    #[cfg(unix)]
    symbols.extend_from_slice(crate::filesystem::WATCH_PORTED_SYMBOLS);
    symbols.extend_from_slice(os_error::stub::PORTED_SYMBOLS);
    symbols.extend_from_slice(crate::process::HostProcess::PORTED_SYMBOLS);
    symbols.extend_from_slice(signal::PORTED_SYMBOLS);
    symbols.extend_from_slice(crate::network::os::PORTED_SYMBOLS);
    symbols.extend_from_slice(crate::network::os::SOCKET_PORTED_SYMBOLS);
    #[cfg(windows)]
    symbols.extend_from_slice(crate::network::os::OVERLAPPED_PORTED_SYMBOLS);
    symbols.extend_from_slice(crate::network::os::interface::PORTED_SYMBOLS);
    symbols.extend_from_slice(crate::network::os::address::PORTED_SYMBOLS);
    #[cfg(unix)]
    symbols.extend_from_slice(crate::network::os::dns::PORTED_SYMBOLS);
    symbols
}

#[cfg(test)]
pub(crate) fn compat_symbols() -> Vec<CompatSymbol> {
    let mut symbols = Vec::new();
    symbols.extend(crate::filesystem::compat_symbols());
    symbols.extend_from_slice(os_error::stub::COMPAT_SYMBOLS);
    symbols
}

#[cfg(test)]
mod tests {
    const UPSTREAM_HELPER: super::PortedSymbol = super::PortedSymbol {
        rust_module: module_path!(),
        rust_symbol: "channel_binding",
        native_symbol: "get_channel_binding",
        root: super::SourceRoot::MoonbitAsync,
        source: "src/tls/schannel.c",
    };

    struct MethodExample;

    ported_fns! {
        impl MethodExample {
            #[ported(source = "src/tls/schannel.c", original = [
                "get_channel_binding",
                "moonbitlang_async_schannel_unique_channel_binding",
            ])]
            fn identity(&self, value: u32) -> u32 {
                value
            }

            #[ported(source = "missing.c", original = ["disabled_a", "disabled_b"])]
            #[cfg(any())]
            fn disabled(&self) {
                unavailable_on_this_platform();
            }
        }
    }

    #[test]
    fn methods_keep_all_sources_and_exclude_disabled_functions() {
        assert_eq!(MethodExample.identity(42), 42);
        let symbols = MethodExample::PORTED_SYMBOLS;
        assert_eq!(symbols.len(), 2);
        assert!(
            symbols
                .iter()
                .all(|symbol| symbol.rust_symbol == "MethodExample::identity")
        );
        assert_eq!(symbols[0].native_symbol, "get_channel_binding");
        assert_eq!(
            symbols[1].native_symbol,
            "moonbitlang_async_schannel_unique_channel_binding"
        );
        for symbol in symbols {
            symbol.assert_source_exists();
        }
    }

    ported_fns! {
        #[ported(source = "src/tls/schannel.c", original = [
            "get_channel_binding",
            "moonbitlang_async_schannel_unique_channel_binding",
        ])]
        /// # Safety
        /// `value` must be valid for reading.
        unsafe fn read_value(value: *const u32) -> u32 {
            unsafe { *value }
        }

        #[ported(source = "missing.c", original = "disabled")]
        #[cfg(any())]
        unsafe fn disabled() {
            unavailable_on_this_platform();
        }
    }

    #[test]
    fn unsafe_functions_keep_all_sources_and_exclude_disabled_functions() {
        let value = 42;
        assert_eq!(unsafe { read_value(&value) }, value);
        assert_eq!(PORTED_SYMBOLS.len(), 2);
        assert!(
            PORTED_SYMBOLS
                .iter()
                .all(|symbol| symbol.rust_symbol == "read_value")
        );
        assert_eq!(PORTED_SYMBOLS[0].native_symbol, "get_channel_binding");
        assert_eq!(
            PORTED_SYMBOLS[1].native_symbol,
            "moonbitlang_async_schannel_unique_channel_binding"
        );
        for symbol in PORTED_SYMBOLS {
            symbol.assert_source_exists();
        }
    }

    #[test]
    fn upstream_helper_does_not_need_a_wasm_import() {
        UPSTREAM_HELPER.assert_source_exists();
    }

    #[test]
    #[should_panic(expected = "does not contain symbol get_channel_bindin")]
    fn source_check_rejects_symbol_prefixes() {
        super::PortedSymbol {
            native_symbol: "get_channel_bindin",
            ..UPSTREAM_HELPER
        }
        .assert_source_exists();
    }

    #[test]
    #[should_panic(expected = "does not contain symbol get_channel_binding")]
    fn source_check_rejects_the_wrong_source_file() {
        super::PortedSymbol {
            source: "src/tls/openssl.c",
            ..UPSTREAM_HELPER
        }
        .assert_source_exists();
    }

    #[test]
    #[should_panic(expected = "failed to read upstream source")]
    fn source_check_rejects_missing_files() {
        super::PortedSymbol {
            source: "src/tls/missing-schannel.c",
            ..UPSTREAM_HELPER
        }
        .assert_source_exists();
    }

    #[test]
    fn ported_implementations_reference_upstream_sources() {
        for symbol in super::ported_symbols() {
            symbol.assert_source_exists();
        }
    }
}
