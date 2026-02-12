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

use crate::v8_builder::{ArgsExt, ObjectExt, ScopeExt};
use std::path::Path;

fn resolve_source_map_path_impl(wasm_path: &str, source_map_path: &str) -> String {
    if source_map_path.is_empty() {
        return String::new();
    }
    if Path::new(source_map_path).is_absolute() {
        return source_map_path.to_string();
    }

    let base_dir = Path::new(wasm_path)
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let joined = base_dir.join(source_map_path);
    joined.to_string_lossy().into_owned()
}

fn resolve_source_map_path(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let wasm_path = args.string_lossy(scope, 0);
    let source_map_path = args.string_lossy(scope, 1);
    let resolved = resolve_source_map_path_impl(&wasm_path, &source_map_path);
    ret.set(scope.string(&resolved).into());
}

const BACKTRACE_RUNTIME_NAMESPACE: &str = "__moonbit_backtrace_runtime";

pub(crate) fn init(scope: &mut v8::HandleScope) {
    let global_proxy = scope.get_current_context().global(scope);
    let backtrace_obj = global_proxy.child(scope, BACKTRACE_RUNTIME_NAMESPACE);

    backtrace_obj.set_func(scope, "resolve_source_map_path", resolve_source_map_path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn resolve_source_map_path_handles_relative_and_absolute_paths_unix() {
        assert_eq!(
            resolve_source_map_path_impl("/repo/main.wasm", "main.wasm.map"),
            "/repo/main.wasm.map"
        );
        assert_eq!(
            resolve_source_map_path_impl("/repo/main.wasm", "/tmp/main.wasm.map"),
            "/tmp/main.wasm.map"
        );
    }

    #[cfg(windows)]
    #[test]
    fn resolve_source_map_path_handles_windows_paths() {
        assert_eq!(
            resolve_source_map_path_impl(r"C:\repo\main.wasm", r"maps\main.wasm.map"),
            r"C:\repo\maps\main.wasm.map"
        );
        assert_eq!(
            resolve_source_map_path_impl(r"C:\repo\main.wasm", r"C:\tmp\main.wasm.map"),
            r"C:\tmp\main.wasm.map"
        );
    }
}
