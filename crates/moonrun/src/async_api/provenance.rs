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

#[cfg(test)]
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceRoot {
    MoonbitAsync,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SourceLocation {
    pub(crate) root: SourceRoot,
    pub(crate) path: &'static str,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PortedImport {
    pub(crate) rust_module: &'static str,
    pub(crate) rust_symbol: &'static str,
    pub(crate) native_symbol: Option<&'static str>,
    pub(crate) sources: &'static [SourceLocation],
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompatImport {
    pub(crate) rust_module: &'static str,
    pub(crate) rust_symbol: &'static str,
    pub(crate) original_symbol: &'static str,
    pub(crate) historical_source: &'static str,
    pub(crate) upstream_pr: u32,
    pub(crate) replacement: &'static str,
    pub(crate) no_op: bool,
    pub(crate) api_only: bool,
}

macro_rules! ported_imports {
    (@collect [$($entries:tt)*] [$($compat_entries:tt)*] [$($out:tt)*]) => {
        #[cfg(test)]
        pub(super) const PORTED_IMPORTS: &[$crate::async_api::provenance::PortedImport] = &[
            $($entries)*
        ];

        #[cfg(test)]
        #[allow(dead_code)]
        pub(super) const COMPAT_IMPORTS: &[$crate::async_api::provenance::CompatImport] = &[
            $($compat_entries)*
        ];

        $($out)*
    };
    (
        @collect [$($entries:tt)*] [$($compat_entries:tt)*] [$($out:tt)*]
        #[ported(source = $source_path:literal, original = $original:literal)]
        #[cfg($($cfg:tt)*)]
        $(#[$meta:meta])*
        $vis:vis fn $function:ident($($params:tt)*) $(-> $ret:ty)? $body:block
        $($rest:tt)*
    ) => {
        ported_imports!(
            @collect [
                $($entries)*
                #[cfg($($cfg)*)]
                $crate::async_api::provenance::PortedImport {
                    rust_module: module_path!(),
                    rust_symbol: stringify!($function),
                    native_symbol: Some($original),
                    sources: &[
                        $crate::async_api::provenance::SourceLocation {
                            root: $crate::async_api::provenance::SourceRoot::MoonbitAsync,
                            path: $source_path,
                        },
                    ],
                },
            ] [$($compat_entries)*] [
                $($out)*
                #[cfg($($cfg)*)]
                $(#[$meta])*
                $vis fn $function($($params)*) $(-> $ret)? $body
            ]
            $($rest)*
        );
    };
    (
        @collect [$($entries:tt)*] [$($compat_entries:tt)*] [$($out:tt)*]
        #[ported(source = $source_path:literal, original = $original:literal)]
        $(#[$meta:meta])*
        $vis:vis fn $function:ident($($params:tt)*) $(-> $ret:ty)? $body:block
        $($rest:tt)*
    ) => {
        ported_imports!(
            @collect [
                $($entries)*
                $crate::async_api::provenance::PortedImport {
                    rust_module: module_path!(),
                    rust_symbol: stringify!($function),
                    native_symbol: Some($original),
                    sources: &[
                        $crate::async_api::provenance::SourceLocation {
                            root: $crate::async_api::provenance::SourceRoot::MoonbitAsync,
                            path: $source_path,
                        },
                    ],
                },
            ] [$($compat_entries)*] [
                $($out)*
                $(#[$meta])*
                $vis fn $function($($params)*) $(-> $ret)? $body
            ]
            $($rest)*
        );
    };
    (
        @collect [$($entries:tt)*] [$($compat_entries:tt)*] [$($out:tt)*]
        #[ported(source = $source_path:literal)]
        #[cfg($($cfg:tt)*)]
        $(#[$meta:meta])*
        $vis:vis fn $function:ident($($params:tt)*) $(-> $ret:ty)? $body:block
        $($rest:tt)*
    ) => {
        ported_imports!(
            @collect [
                $($entries)*
                #[cfg($($cfg)*)]
                $crate::async_api::provenance::PortedImport {
                    rust_module: module_path!(),
                    rust_symbol: stringify!($function),
                    native_symbol: None,
                    sources: &[
                        $crate::async_api::provenance::SourceLocation {
                            root: $crate::async_api::provenance::SourceRoot::MoonbitAsync,
                            path: $source_path,
                        },
                    ],
                },
            ] [$($compat_entries)*] [
                $($out)*
                #[cfg($($cfg)*)]
                $(#[$meta])*
                $vis fn $function($($params)*) $(-> $ret)? $body
            ]
            $($rest)*
        );
    };
    (
        @collect [$($entries:tt)*] [$($compat_entries:tt)*] [$($out:tt)*]
        #[ported(source = $source_path:literal)]
        $(#[$meta:meta])*
        $vis:vis fn $function:ident($($params:tt)*) $(-> $ret:ty)? $body:block
        $($rest:tt)*
    ) => {
        ported_imports!(
            @collect [
                $($entries)*
                $crate::async_api::provenance::PortedImport {
                    rust_module: module_path!(),
                    rust_symbol: stringify!($function),
                    native_symbol: None,
                    sources: &[
                        $crate::async_api::provenance::SourceLocation {
                            root: $crate::async_api::provenance::SourceRoot::MoonbitAsync,
                            path: $source_path,
                        },
                    ],
                },
            ] [$($compat_entries)*] [
                $($out)*
                $(#[$meta])*
                $vis fn $function($($params)*) $(-> $ret)? $body
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
            replacement = $replacement:literal,
            no_op = true
        )]
        $(#[$meta:meta])*
        $vis:vis fn $name:ident($($args:tt)*) $(-> $ret:ty)? $body:block
        $($rest:tt)*
    ) => {
        ported_imports!(
            @collect [$($entries)*] [
                $($compat_entries)*
                $crate::async_api::provenance::CompatImport {
                    rust_module: module_path!(),
                    rust_symbol: stringify!($name),
                    original_symbol: $original,
                    historical_source: $source,
                    upstream_pr: $upstream_pr,
                    replacement: $replacement,
                    no_op: true,
                    api_only: false,
                },
            ] [
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
            replacement = $replacement:literal,
            api_only = true
        )]
        $(#[$meta:meta])*
        $vis:vis fn $name:ident($($args:tt)*) $(-> $ret:ty)? $body:block
        $($rest:tt)*
    ) => {
        ported_imports!(
            @collect [$($entries)*] [
                $($compat_entries)*
                $crate::async_api::provenance::CompatImport {
                    rust_module: module_path!(),
                    rust_symbol: stringify!($name),
                    original_symbol: $original,
                    historical_source: $source,
                    upstream_pr: $upstream_pr,
                    replacement: $replacement,
                    no_op: false,
                    api_only: true,
                },
            ] [
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
        ported_imports!(
            @collect [$($entries)*] [
                $($compat_entries)*
                $crate::async_api::provenance::CompatImport {
                    rust_module: module_path!(),
                    rust_symbol: stringify!($name),
                    original_symbol: $original,
                    historical_source: $source,
                    upstream_pr: $upstream_pr,
                    replacement: $replacement,
                    no_op: false,
                    api_only: false,
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
        ported_imports!(@collect [$($entries)*] [$($compat_entries)*] [$($out)* $item] $($rest)*);
    };
    ($($items:tt)*) => {
        ported_imports!(@collect [] [] [] $($items)*);
    };
}

pub(super) use ported_imports;
