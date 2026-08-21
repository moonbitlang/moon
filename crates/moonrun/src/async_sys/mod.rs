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

//! V8-free async host operations ported from `moonbitlang/async` native stubs.
//!
//! Files under this module follow the async package's source layout where a
//! wasm import has a live implementation. The provenance macros record the
//! native source path and symbol so the adapter registry can stay aligned with
//! the C implementation. Moonrun-only runtime state stays in `async_host`.

pub(crate) mod fs;
pub(crate) mod internal;
pub(crate) mod os_error;
pub(crate) mod process;
pub(crate) mod signal;
pub(crate) mod socket;

#[cfg(test)]
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PortedSymbol {
    pub(crate) rust_module: &'static str,
    pub(crate) rust_symbol: &'static str,
    pub(crate) native_symbol: &'static str,
    pub(crate) source: &'static str,
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

macro_rules! ported_fns {
    (@collect [$($entries:tt)*] [$($compat_entries:tt)*] [$($out:tt)*]) => {
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
        @collect [$($entries:tt)*] [$($compat_entries:tt)*] [$($out:tt)*]
        #[ported(source = $source:literal, original = $original:literal)]
        #[cfg($($cfg:tt)*)]
        $(#[$meta:meta])*
        $vis:vis fn $name:ident($($args:tt)*) $(-> $ret:ty)? $body:block
        $($rest:tt)*
    ) => {
        ported_fns!(
            @collect [
                $($entries)*
                #[cfg($($cfg)*)]
                crate::async_sys::PortedSymbol {
                    rust_module: module_path!(),
                    rust_symbol: stringify!($name),
                    native_symbol: $original,
                    source: $source,
                },
            ] [$($compat_entries)*] [
                $($out)*
                #[cfg($($cfg)*)]
                $(#[$meta])*
                $vis fn $name($($args)*) $(-> $ret)? $body
            ]
            $($rest)*
        );
    };
    (
        @collect [$($entries:tt)*] [$($compat_entries:tt)*] [$($out:tt)*]
        #[ported(source = $source:literal, original = $original:literal)]
        $(#[$meta:meta])*
        $vis:vis fn $name:ident($($args:tt)*) $(-> $ret:ty)? $body:block
        $($rest:tt)*
    ) => {
        ported_fns!(
            @collect [
                $($entries)*
                crate::async_sys::PortedSymbol {
                    rust_module: module_path!(),
                    rust_symbol: stringify!($name),
                    native_symbol: $original,
                    source: $source,
                },
            ] [$($compat_entries)*] [
                $($out)*
                $(#[$meta])*
                $vis fn $name($($args)*) $(-> $ret)? $body
            ]
            $($rest)*
        );
    };
    (
        @collect [$($entries:tt)*] [$($compat_entries:tt)*] [$($out:tt)*]
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
            @collect [$($entries)*] [
                $($compat_entries)*
                crate::async_sys::CompatSymbol {
                    rust_module: module_path!(),
                    rust_symbol: stringify!($name),
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
    (@collect [$($entries:tt)*] [$($compat_entries:tt)*] [$($out:tt)*] $item:item $($rest:tt)*) => {
        ported_fns!(@collect [$($entries)*] [$($compat_entries)*] [$($out)* $item] $($rest)*);
    };
    ($($items:tt)*) => {
        ported_fns!(@collect [] [] [] $($items)*);
    };
}

pub(crate) use ported_fns;

#[cfg(test)]
pub(crate) fn ported_symbols() -> Vec<PortedSymbol> {
    let mut symbols = Vec::new();
    symbols.extend_from_slice(internal::c_buffer::stub::PORTED_SYMBOLS);
    symbols.extend_from_slice(internal::env_util::stub::PORTED_SYMBOLS);
    symbols.extend_from_slice(internal::fd_util::stub::PORTED_SYMBOLS);
    symbols.extend(internal::event_loop::io::ported_symbols());
    symbols.extend(internal::event_loop::poll::ported_symbols());
    symbols.extend(internal::event_loop::thread_pool::ported_symbols());
    symbols.extend_from_slice(fs::dir::PORTED_SYMBOLS);
    symbols.extend_from_slice(fs::stub::PORTED_SYMBOLS);
    #[cfg(target_os = "linux")]
    symbols.extend_from_slice(fs::watch_inotify::PORTED_SYMBOLS);
    #[cfg(target_os = "macos")]
    symbols.extend_from_slice(fs::watch_kqueue::PORTED_SYMBOLS);
    #[cfg(windows)]
    symbols.extend_from_slice(fs::watch_windows::PORTED_SYMBOLS);
    symbols.extend_from_slice(os_error::stub::PORTED_SYMBOLS);
    symbols.extend_from_slice(process::PORTED_SYMBOLS);
    symbols.extend_from_slice(signal::PORTED_SYMBOLS);
    symbols.extend_from_slice(socket::PORTED_SYMBOLS);
    symbols
}

#[cfg(test)]
pub(crate) fn compat_symbols() -> Vec<CompatSymbol> {
    let mut symbols = Vec::new();
    symbols.extend(crate::filesystem::compat_symbols());
    symbols.extend_from_slice(internal::fd_util::stub::COMPAT_SYMBOLS);
    symbols
}
