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

//! Native observations used while preparing executable lookup.
//!
//! Runtime input selection and candidate generation belong to Host Process.
//! These operations inspect only the supplied plan or named native facilities;
//! they never repeat PATH lookup or select authorization policy.

#[cfg(windows)]
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::PathBuf;

use super::super::lookup::LookupPlan;

/// Select a conventional executable Moon tool from captured candidates.
/// Preparation cannot run several helpers to discover a usable interpreter.
#[cfg(unix)]
pub(in crate::process) fn resolve_for_moon(plan: &LookupPlan) -> io::Result<PathBuf> {
    use std::os::unix::ffi::OsStrExt;

    let mut denied = false;
    for candidate in plan.candidates() {
        let error = match std::fs::metadata(candidate) {
            Ok(metadata) if metadata.is_file() => {
                let path = std::ffi::CString::new(candidate.as_os_str().as_bytes())
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
                if unsafe { libc::access(path.as_ptr(), libc::X_OK) } == 0 {
                    return Ok(candidate.clone());
                }
                io::Error::last_os_error()
                    .raw_os_error()
                    .unwrap_or(libc::EIO)
            }
            Ok(_) => libc::EACCES,
            Err(error) => error.raw_os_error().unwrap_or(libc::EIO),
        };
        if !plan.is_search() {
            return Err(io::Error::from_raw_os_error(error));
        }
        match error {
            libc::EACCES => denied = true,
            libc::ENOENT | libc::ENOTDIR => {}
            error => return Err(io::Error::from_raw_os_error(error)),
        }
    }
    Err(io::Error::from_raw_os_error(if denied {
        libc::EACCES
    } else {
        libc::ENOENT
    }))
}

/// Parse native Windows arguments without losing unpaired UTF-16 surrogates.
#[cfg(windows)]
fn split_windows_os(args: &std::ffi::OsStr) -> std::io::Result<Vec<std::ffi::OsString>> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let mut wide: Vec<_> = args.encode_wide().collect();
    // CommandLineToArgvW treats empty input as the host executable. That is not
    // a guest spawn request, and embedded NULs must not truncate the request.
    if wide.is_empty() || wide.contains(&0) {
        return Err(std::io::Error::from_raw_os_error(
            windows_sys::Win32::Foundation::ERROR_INVALID_PARAMETER as i32,
        ));
    }
    wide.push(0);
    let mut count = 0;
    // The API owns one allocation containing the pointer array and terminated
    // arguments. Copy the code units before releasing that whole allocation.
    let argv =
        unsafe { windows_sys::Win32::UI::Shell::CommandLineToArgvW(wide.as_ptr(), &mut count) };
    if argv.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    let result = unsafe { std::slice::from_raw_parts(argv, count as usize) }
        .iter()
        .map(|arg| {
            let mut len = 0;
            while unsafe { *(*arg).add(len) } != 0 {
                len += 1;
            }
            std::ffi::OsString::from_wide(unsafe { std::slice::from_raw_parts(*arg, len) })
        })
        .collect();
    unsafe { windows_sys::Win32::Foundation::LocalFree(argv.cast()) };
    Ok(result)
}

#[cfg(windows)]
pub(in crate::process) fn parse_command_line(command_line: &OsStr) -> io::Result<Vec<OsString>> {
    use std::os::windows::ffi::OsStrExt;

    // Empty input would make CommandLineToArgvW return the host executable.
    // Embedded NULs would silently truncate the guest's request.
    if command_line.is_empty() || command_line.encode_wide().any(|unit| unit == 0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the process command line is empty or contains NUL",
        ));
    }
    let arguments = split_windows_os(command_line)?;
    if arguments.first().is_none_or(|program| program.is_empty()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the process executable name is empty",
        ));
    }
    Ok(arguments)
}

/// Observe native search directories without consulting either environment.
/// Host Process inserts the Runtime cwd between the application and OS paths.
#[cfg(windows)]
pub(in crate::process) fn windows_directories() -> io::Result<(Option<PathBuf>, Vec<PathBuf>)> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::SystemInformation::{
        GetSystemDirectoryW, GetWindowsDirectoryW,
    };

    let application = std::env::current_exe()?;
    let application_directory = application.parent().map(ToOwned::to_owned);
    // Both APIs report the required capacity on overflow. Their results are
    // explicit paths; SystemRoot in the child environment is not a fallback.
    let mut directories = Vec::new();
    for get_directory in [GetSystemDirectoryW, GetWindowsDirectoryW] {
        let mut buffer = vec![0u16; 260];
        loop {
            let length = unsafe { get_directory(buffer.as_mut_ptr(), buffer.len() as u32) };
            if length == 0 {
                return Err(io::Error::last_os_error());
            }
            if (length as usize) < buffer.len() {
                buffer.truncate(length as usize);
                directories.push(PathBuf::from(OsString::from_wide(&buffer)));
                break;
            }
            buffer.resize(length as usize + 1, 0);
        }
    }
    let windows_directory = directories.pop().unwrap();
    directories.push(windows_directory.join("System"));
    directories.push(windows_directory);
    Ok((application_directory, directories))
}

#[cfg(windows)]
pub(in crate::process) fn select_image(plan: &LookupPlan) -> io::Result<PathBuf> {
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND};

    for path in plan.candidates() {
        match std::fs::metadata(path) {
            // Select directories too: CreateProcessW owns the error for an
            // existing object that cannot be loaded as an executable.
            Ok(_) => return Ok(path.clone()),
            Err(error)
                if matches!(
                    error.raw_os_error(),
                    Some(code) if code == ERROR_FILE_NOT_FOUND as i32
                        || code == ERROR_PATH_NOT_FOUND as i32
                ) => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::from_raw_os_error(ERROR_FILE_NOT_FOUND as i32))
}

#[cfg(all(test, windows))]
mod tests {
    use super::split_windows_os;

    #[test]
    fn native_windows_arguments_preserve_unpaired_surrogates() {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        let program: Vec<_> = "C:\\Moon "
            .encode_utf16()
            .chain([0xd800])
            .chain("\\moon.exe".encode_utf16())
            .collect();
        let argument = [b'x' as u16, 0xdc00, b' ' as u16, b'y' as u16];
        let command = [b'"' as u16]
            .into_iter()
            .chain(program.iter().copied())
            .chain("\" \"".encode_utf16())
            .chain(argument)
            .chain([b'"' as u16])
            .collect::<Vec<_>>();
        assert_eq!(
            split_windows_os(&OsString::from_wide(&command)).unwrap(),
            [
                OsString::from_wide(&program),
                OsString::from_wide(&argument)
            ]
        );
        for value in ["", "moon\0ignored"] {
            assert!(split_windows_os(value.as_ref()).is_err());
        }
    }
}
