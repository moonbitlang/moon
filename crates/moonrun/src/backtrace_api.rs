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
use std::io::IsTerminal;
use std::path::{Component, Path, PathBuf};

pub fn should_use_backtrace_color() -> bool {
    if let Ok(explicit) = std::env::var("MOONBIT_BACKTRACE_COLOR") {
        match explicit.as_str() {
            "0" | "false" | "never" => return false,
            "1" | "true" | "always" => return true,
            _ => {}
        }
    }

    if let Ok(no_color) = std::env::var("NO_COLOR") {
        if !no_color.is_empty() {
            return false;
        }
    }

    std::io::stderr().is_terminal()
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    let absolute = path.is_absolute();

    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() && !absolute {
                    out.push("..");
                }
            }
            Component::RootDir | Component::Prefix(_) | Component::Normal(_) => {
                out.push(comp.as_os_str());
            }
        }
    }

    if out.as_os_str().is_empty() && !absolute {
        PathBuf::from(".")
    } else {
        out
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn format_source_path_auto_impl(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    let source = normalize_path(Path::new(path));
    let normalized = display_path(&source);
    if !source.is_absolute() {
        return normalized;
    }

    let Ok(cwd) = std::env::current_dir() else {
        return normalized;
    };
    let cwd = normalize_path(&cwd);
    let Ok(rel) = source.strip_prefix(&cwd) else {
        return normalized;
    };
    if rel.as_os_str().is_empty() {
        return normalized;
    }
    display_path(rel)
}

fn format_source_path_auto(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.string_lossy(scope, 0);
    let formatted = format_source_path_auto_impl(&path);
    let formatted = scope.string(&formatted);
    ret.set(formatted.into());
}

pub fn init_backtrace<'s>(obj: v8::Local<'s, v8::Object>, scope: &mut v8::HandleScope<'s>) {
    obj.set_func(scope, "format_source_path_auto", format_source_path_auto);
}

#[cfg(test)]
mod tests {
    use super::{display_path, format_source_path_auto_impl, normalize_path};
    use std::path::Path;

    #[test]
    fn normalize_keeps_semantics() {
        assert_eq!(
            display_path(&normalize_path(Path::new("./a/./b/../c"))),
            "a/c"
        );
        assert_eq!(
            display_path(&normalize_path(Path::new("/a/./b/../c"))),
            "/a/c"
        );
    }

    #[test]
    fn relative_path_stays_relative() {
        assert_eq!(format_source_path_auto_impl("a/b/c.mbt"), "a/b/c.mbt");
    }

    #[test]
    fn path_inside_cwd_becomes_relative() {
        let cwd = std::env::current_dir().unwrap();
        let path = cwd.join("foo/bar.mbt");
        assert_eq!(
            format_source_path_auto_impl(path.to_string_lossy().as_ref()),
            "foo/bar.mbt"
        );
    }
}
