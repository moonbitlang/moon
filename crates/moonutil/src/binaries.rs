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

use std::path::PathBuf;

macro_rules! define_binaries {
    // Entry point - normalize by adding trailing commas
    (
        $($field:ident: $kind:ident($($args:tt)*)),* $(,)?
    ) => {
        define_binaries! {
            @munch
            []  // moon_bin accumulator
            []  // which_bin accumulator
            [ $($field: $kind($($args)*),)* ]  // items to process
        }
    };

    // Munch moon_bin entry
    (
        @munch
        [$($moon:tt)*]
        [$($which:tt)*]
        [$field:ident: moon_bin($bin_name:literal, $override_env:literal), $($rest:tt)*]
    ) => {
        define_binaries! {
            @munch
            [$($moon)* @moon { $field, $bin_name, $override_env }]
            [$($which)*]
            [$($rest)*]
        }
    };

    // Munch which_bin entry
    (
        @munch
        [$($moon:tt)*]
        [$($which:tt)*]
        [$field:ident: which_bin([$($which_names:literal),+ $(,)?], $which_override_env:literal), $($rest:tt)*]
    ) => {
        define_binaries! {
            @munch
            [$($moon)*]
            [$($which)* @which { $field, [$($which_names),+], $which_override_env }]
            [$($rest)*]
        }
    };

    // Done munching, generate code
    (
        @munch
        [$($moon:tt)*]
        [$($which:tt)*]
        []
    ) => {
        define_binaries! {
            @generate
            [$($moon)*]
            [$($which)*]
        }
    };

    // Generate the struct and static
    (
        @generate
        [$( @moon { $field:ident, $bin_name:literal, $override_env:literal } )*]
        [$( @which { $which_field:ident, [$($which_names:literal),+], $which_override_env:literal } )*]
    ) => {
        pub struct CachedBinaries {
            $(
                pub $field: std::sync::LazyLock<PathBuf>,
            )*
            $(
                pub $which_field: std::sync::LazyLock<Option<PathBuf>>,
            )*
        }

        pub static BINARIES: CachedBinaries = CachedBinaries {
            $(
                $field: std::sync::LazyLock::new(|| {
                    if let Some(path) = std::env::var_os($override_env) {
                        return PathBuf::from(path);
                    }
                    let path = crate::moon_dir::bin().join($bin_name);
                    #[cfg(target_os = "windows")]
                    let path = if path.extension().is_none() {
                        path.with_extension("exe")
                    } else {
                        path
                    };
                    path
                }),
            )*
            $(
                $which_field: std::sync::LazyLock::new(|| {
                    if let Some(custom_path) = std::env::var_os($which_override_env) {
                        return Some(PathBuf::from(custom_path));
                    }
                    [$($which_names),+]
                        .iter()
                        .find_map(|name| which::which(name).ok())
                }),
            )*
        };

        impl CachedBinaries {
            $(
                pub fn $which_field(&self) -> PathBuf {
                    self.$which_field.clone().unwrap_or_else(|| {
                        let path = PathBuf::from(stringify!($which_field));
                        #[cfg(target_os = "windows")]
                        let path = if path.extension().is_none() {
                            path.with_extension("exe")
                        } else {
                            path
                        };
                        path
                    })
                }
            )*
        }
    };
}

define_binaries! {
    moonbuild: moon_bin("moon", "MOON_OVERRIDE"),
    moonc: moon_bin("moonc", "MOONC_OVERRIDE"),
    mooncake: moon_bin("mooncake", "MOONCAKE_OVERRIDE"),
    moondoc: moon_bin("moondoc", "MOONDOC_OVERRIDE"),
    moonfmt: moon_bin("moonfmt", "MOONFMT_OVERRIDE"),
    mooninfo: moon_bin("mooninfo", "MOONINFO_OVERRIDE"),
    moonlex: moon_bin("moonlex.wasm", "MOONLEX_OVERRIDE"),
    moonrun: moon_bin("moonrun", "MOONRUN_OVERRIDE"),
    moonyacc: moon_bin("moonyacc.wasm", "MOONYACC_OVERRIDE"),
    moon_cove_report: moon_bin("moon_cove_report", "MOON_COVE_REPORT_OVERRIDE"),
    node: which_bin(["node.cmd", "node"], "MOON_NODE_OVERRIDE"),
    python: which_bin(["python", "python3"], "MOON_PYTHON_OVERRIDE"),
    git: which_bin(["git"], "MOON_GIT_OVERRIDE"),
}
