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

#[cfg(unix)]
mod unix {
    use std::{
        ffi::{CStr, CString, c_char, c_int, c_void},
        os::unix::ffi::OsStrExt,
        ptr,
    };

    type MainFn = unsafe extern "C" fn(c_int, *mut *mut c_char) -> c_int;

    pub fn run() -> i32 {
        let guest_args = match guest_args() {
            Ok(args) => args,
            Err(message) => {
                eprintln!("{message}");
                return 1;
            }
        };

        let dylib_path = &guest_args[0];
        let handle =
            unsafe { libc::dlopen(dylib_path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            eprintln!(
                "moon-native-runner: failed to load `{}`: {}",
                dylib_path.to_string_lossy(),
                dlerror_message(),
            );
            return 1;
        }

        let symbol_name = c"main";
        let symbol = unsafe { libc::dlsym(handle, symbol_name.as_ptr()) };
        if symbol.is_null() {
            eprintln!(
                "moon-native-runner: failed to find `main` in `{}`: {}",
                dylib_path.to_string_lossy(),
                dlerror_message(),
            );
            return 1;
        }

        let mut argv = guest_args
            .iter()
            .map(|arg| arg.as_ptr() as *mut c_char)
            .chain(std::iter::once(ptr::null_mut()))
            .collect::<Vec<_>>();
        let argc = i32::try_from(guest_args.len()).unwrap_or(i32::MAX);
        let main_fn = unsafe { std::mem::transmute::<*mut c_void, MainFn>(symbol) };
        unsafe { main_fn(argc, argv.as_mut_ptr()) }
    }

    fn guest_args() -> Result<Vec<CString>, String> {
        let args = std::env::args_os().skip(1).collect::<Vec<_>>();
        if args.is_empty() {
            return Err("usage: moon-native-runner <dylib> [args...]".to_string());
        }

        args.into_iter()
            .map(|arg| {
                CString::new(arg.as_os_str().as_bytes()).map_err(|_| {
                    format!(
                        "moon-native-runner: argument contains an interior NUL byte: `{}`",
                        arg.to_string_lossy(),
                    )
                })
            })
            .collect()
    }

    fn dlerror_message() -> String {
        let error = unsafe { libc::dlerror() };
        if error.is_null() {
            "unknown dynamic loader error".to_string()
        } else {
            unsafe { CStr::from_ptr(error) }
                .to_string_lossy()
                .into_owned()
        }
    }
}

#[cfg(unix)]
fn main() {
    std::process::exit(unix::run());
}

#[cfg(not(unix))]
fn main() {
    eprintln!("moon-native-runner: native dylib runner is unsupported on this platform");
    std::process::exit(1);
}
