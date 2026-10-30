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

//! Executable lookup interpreted from the spawning Runtime's inputs.
//!
//! Unix freezes candidates, not an `access()` prediction of which `exec` will
//! succeed. The native runner retains platform-specific error and script
//! fallback behavior without consulting PATH again. Windows selects an image
//! separately from the original command line passed to CreateProcessW.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::async_host::{AsyncHostError, AsyncHostResult};

#[cfg(unix)]
#[derive(Debug)]
pub(super) struct Executable {
    program: OsString,
    candidates: Vec<PathBuf>,
    search: bool,
}

#[cfg(unix)]
impl Executable {
    pub(super) fn new(
        program: &OsStr,
        parent_path: Option<&OsStr>,
        child_cwd: &Path,
    ) -> AsyncHostResult<Self> {
        use std::os::unix::ffi::OsStrExt;

        if program.is_empty() {
            return Err(AsyncHostError::Native(libc::ENOENT));
        }
        if !child_cwd.is_absolute()
            || program.as_bytes().contains(&0)
            || child_cwd.as_os_str().as_bytes().contains(&0)
            || parent_path.is_some_and(|path| path.as_bytes().contains(&0))
        {
            return Err(AsyncHostError::Inval);
        }
        let search = !program.as_bytes().contains(&b'/');
        let candidates = if search {
            // These are the native spawn search defaults, not the shell's
            // login PATH or confstr(_CS_PATH)'s list of standard utilities.
            #[cfg(target_os = "macos")]
            let default_path = OsStr::new("/usr/bin:/bin");
            #[cfg(all(target_os = "linux", target_env = "musl"))]
            let default_path = OsStr::new("/usr/local/bin:/bin:/usr/bin");
            #[cfg(not(any(target_os = "macos", all(target_os = "linux", target_env = "musl"))))]
            let default_path = OsStr::new("/bin:/usr/bin");

            parent_path
                .unwrap_or(default_path)
                .as_bytes()
                .split(|byte| *byte == b':')
                .map(|directory| child_cwd.join(OsStr::from_bytes(directory)).join(program))
                .collect()
        } else {
            vec![child_cwd.join(program)]
        };
        Ok(Self {
            program: program.to_owned(),
            candidates,
            search,
        })
    }

    pub(super) fn program(&self) -> &OsStr {
        &self.program
    }

    pub(super) fn candidates(&self) -> &[PathBuf] {
        &self.candidates
    }

    pub(super) fn is_search(&self) -> bool {
        self.search
    }

    /// Select a conventional executable Moon tool from the already captured
    /// candidates. Unlike native spawn, preparation cannot try running several
    /// tools to discover which one has a usable interpreter or binary format.
    pub(super) fn resolve_for_moon(&self) -> AsyncHostResult<PathBuf> {
        use std::os::unix::ffi::OsStrExt;

        let mut denied = false;
        for candidate in &self.candidates {
            let error = match std::fs::metadata(candidate) {
                Ok(metadata) if metadata.is_file() => {
                    let path = std::ffi::CString::new(candidate.as_os_str().as_bytes())
                        .map_err(|_| AsyncHostError::Inval)?;
                    if unsafe { libc::access(path.as_ptr(), libc::X_OK) } == 0 {
                        return Ok(candidate.clone());
                    }
                    std::io::Error::last_os_error()
                        .raw_os_error()
                        .unwrap_or(libc::EIO)
                }
                Ok(_) => libc::EACCES,
                Err(error) => error.raw_os_error().unwrap_or(libc::EIO),
            };
            if !self.search {
                return Err(AsyncHostError::Native(error));
            }
            match error {
                libc::EACCES => denied = true,
                libc::ENOENT | libc::ENOTDIR => {}
                error => return Err(AsyncHostError::Native(error)),
            }
        }
        Err(AsyncHostError::Native(if denied {
            libc::EACCES
        } else {
            libc::ENOENT
        }))
    }
}

#[cfg(windows)]
#[derive(Debug)]
pub(super) struct Executable {
    program: OsString,
    arguments: Vec<OsString>,
    path: PathBuf,
}

#[cfg(windows)]
impl Executable {
    pub(super) fn new(
        command_line: &OsStr,
        parent_path: Option<&OsStr>,
        parent_cwd: &Path,
        search_current_directory: bool,
    ) -> AsyncHostResult<Self> {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND};
        use windows_sys::Win32::System::SystemInformation::{
            GetSystemDirectoryW, GetWindowsDirectoryW,
        };

        // Ordinary native invocations quote an executable token containing
        // spaces. CreateProcessW's successive guesses for an ambiguous,
        // unquoted executable prefix are deliberately not reproduced here.
        if command_line.is_empty() || command_line.encode_wide().any(|unit| unit == 0) {
            return Err(AsyncHostError::Inval);
        }
        let mut arguments = moonutil::shlex::split_windows_os(command_line).map_err(io_error)?;
        let program = arguments.remove(0);
        if program.is_empty() {
            return Err(AsyncHostError::Inval);
        }
        let mut directories = Vec::new();
        if Path::new(&program).components().count() == 1 {
            // These remain native host inputs: the embedding application
            // directory and the OS directories. Runtime PATH, cwd, and its
            // current-directory search setting are supplied explicitly. In
            // particular, child env/cwd do not replace the parent's lookup.
            let application = std::env::current_exe().map_err(io_error)?;
            if let Some(directory) = application.parent() {
                directories.push(directory.to_owned());
            }
            if search_current_directory {
                directories.push(parent_cwd.to_owned());
            }
            // Both APIs return the required capacity on overflow. Keep their
            // results as paths rather than consulting SystemRoot from either
            // the child environment or a later worker.
            let mut os_directories = Vec::new();
            for get_directory in [GetSystemDirectoryW, GetWindowsDirectoryW] {
                let mut buffer = vec![0u16; 260];
                loop {
                    let length = unsafe { get_directory(buffer.as_mut_ptr(), buffer.len() as u32) };
                    if length == 0 {
                        return Err(io_error(std::io::Error::last_os_error()));
                    }
                    if (length as usize) < buffer.len() {
                        buffer.truncate(length as usize);
                        os_directories.push(PathBuf::from(OsString::from_wide(&buffer)));
                        break;
                    }
                    buffer.resize(length as usize + 1, 0);
                }
            }
            let windows_directory = os_directories.pop().unwrap();
            directories.extend(os_directories);
            directories.push(windows_directory.join("System"));
            directories.push(windows_directory);
        }
        let candidates = windows_candidates(&program, parent_path, parent_cwd, directories)?;
        for path in candidates {
            match std::fs::metadata(&path) {
                // Select directories too: CreateProcessW owns the error for an
                // existing object that cannot be loaded as an executable.
                Ok(_) => {
                    return Ok(Self {
                        program,
                        arguments,
                        path,
                    });
                }
                Err(error)
                    if matches!(
                        error.raw_os_error(),
                        Some(code) if code == ERROR_FILE_NOT_FOUND as i32
                            || code == ERROR_PATH_NOT_FOUND as i32
                    ) => {}
                Err(error) => return Err(io_error(error)),
            }
        }
        Err(AsyncHostError::Native(ERROR_FILE_NOT_FOUND as i32))
    }

    pub(super) fn program(&self) -> &OsStr {
        &self.program
    }

    pub(super) fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn resolve_for_moon(&self) -> AsyncHostResult<PathBuf> {
        Ok(self.path.clone())
    }
}

#[cfg(windows)]
fn windows_candidates(
    program: &OsStr,
    parent_path: Option<&OsStr>,
    parent_cwd: &Path,
    mut directories: Vec<PathBuf>,
) -> AsyncHostResult<Vec<PathBuf>> {
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Component, Prefix};

    if program.is_empty()
        || !parent_cwd.is_absolute()
        || program.encode_wide().any(|unit| unit == 0)
        || parent_cwd.as_os_str().encode_wide().any(|unit| unit == 0)
        || parent_path.is_some_and(|path| path.encode_wide().any(|unit| unit == 0))
    {
        return Err(AsyncHostError::Inval);
    }
    let path = Path::new(program);
    // Drive-relative and rooted-without-drive paths need a Runtime-owned
    // per-drive directory model. Device paths also have different normalization
    // rules. Reject them here instead of letting GetFullPathNameW or path join
    // silently consult the embedding process's drive state. UNC and ordinary
    // fully qualified drive paths are supported.
    match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(_) if path.is_absolute() => {}
            Prefix::UNC(_, _) => {}
            _ => return Err(AsyncHostError::Inval),
        },
        Some(Component::RootDir) => return Err(AsyncHostError::Inval),
        _ => {}
    }
    if path.is_absolute() {
        return Ok(vec![path.to_owned()]);
    }
    if program
        .encode_wide()
        .any(|unit| unit == b'\\' as u16 || unit == b'/' as u16)
    {
        return Ok(vec![parent_cwd.join(path)]);
    }
    // CreateProcess executable lookup is not cmd.exe/PATHEXT lookup. Only a
    // bare name without an extension receives .exe; a trailing dot suppresses
    // that default. Native execution retains the original command line, so
    // selecting the image does not rewrite argv[0] or quote/parse its arguments.
    let mut filename = program.to_owned();
    if path.extension().is_none() && program.as_encoded_bytes().last() != Some(&b'.') {
        filename.push(".exe");
    }
    if let Some(paths) = parent_path {
        for directory in std::env::split_paths(paths) {
            match directory.components().next() {
                Some(Component::Prefix(prefix)) => match prefix.kind() {
                    Prefix::Disk(_) if directory.is_absolute() => {}
                    Prefix::UNC(_, _) => {}
                    _ => return Err(AsyncHostError::Inval),
                },
                Some(Component::RootDir) => return Err(AsyncHostError::Inval),
                _ => {}
            }
            directories.push(parent_cwd.join(directory));
        }
    }
    Ok(directories
        .into_iter()
        .map(|directory| directory.join(&filename))
        .collect())
}

#[cfg(windows)]
fn io_error(error: std::io::Error) -> AsyncHostError {
    error
        .raw_os_error()
        .map_or(AsyncHostError::Io, AsyncHostError::Native)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn unix_lookup_freezes_relative_and_empty_path_entries_without_probing() {
        let executable = Executable::new(
            OsStr::new("does-not-have-to-exist"),
            Some(OsStr::new("relative::/absolute:~/literal")),
            Path::new("/child"),
        )
        .unwrap();
        assert!(executable.is_search());
        assert_eq!(executable.program(), "does-not-have-to-exist");
        assert_eq!(
            executable.candidates(),
            [
                PathBuf::from("/child/relative/does-not-have-to-exist"),
                PathBuf::from("/child/does-not-have-to-exist"),
                PathBuf::from("/absolute/does-not-have-to-exist"),
                PathBuf::from("/child/~/literal/does-not-have-to-exist"),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_explicit_paths_do_not_search_or_canonicalize() {
        for (program, expected) in [
            ("bin/../tool", "/child/bin/../tool"),
            ("/absolute/tool", "/absolute/tool"),
        ] {
            let executable = Executable::new(
                OsStr::new(program),
                Some(OsStr::new("/other")),
                Path::new("/child"),
            )
            .unwrap();
            assert!(!executable.is_search());
            assert_eq!(executable.candidates(), [PathBuf::from(expected)]);
        }
        assert_eq!(
            Executable::new(OsStr::new(""), None, Path::new("/child")).unwrap_err(),
            AsyncHostError::Native(libc::ENOENT)
        );
        assert_eq!(
            Executable::new(OsStr::new("tool"), None, Path::new("relative")).unwrap_err(),
            AsyncHostError::Inval
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_empty_path_searches_cwd_but_missing_path_uses_the_platform_default() {
        let empty = Executable::new(
            OsStr::new("tool"),
            Some(OsStr::new("")),
            Path::new("/child"),
        )
        .unwrap();
        assert_eq!(empty.candidates(), [PathBuf::from("/child/tool")]);
        let missing = Executable::new(OsStr::new("tool"), None, Path::new("/child")).unwrap();
        assert!(missing.candidates().contains(&PathBuf::from("/bin/tool")));
        assert!(
            missing
                .candidates()
                .contains(&PathBuf::from("/usr/bin/tool"))
        );
        assert!(!missing.candidates().contains(&PathBuf::from("/child/tool")));
    }

    #[cfg(unix)]
    #[test]
    fn unix_lookup_preserves_native_bytes() {
        use std::os::unix::ffi::OsStrExt;
        let executable = Executable::new(
            OsStr::from_bytes(b"tool\xff"),
            Some(OsStr::from_bytes(b"bin\xfe")),
            Path::new("/child"),
        )
        .unwrap();
        assert_eq!(
            executable.candidates()[0].as_os_str().as_bytes(),
            b"/child/bin\xfe/tool\xff"
        );
        assert_eq!(
            Executable::new(OsStr::from_bytes(b"tool\0bad"), None, Path::new("/child"))
                .unwrap_err(),
            AsyncHostError::Inval
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_lookup_uses_parent_inputs_and_only_exe_default() {
        let candidates = windows_candidates(
            OsStr::new("tool"),
            Some(OsStr::new("bin;;D:\\tools")),
            Path::new("C:\\parent"),
            vec![
                PathBuf::from("C:\\application"),
                PathBuf::from("C:\\parent"),
            ],
        )
        .unwrap();
        assert_eq!(
            candidates,
            [
                PathBuf::from("C:\\application\\tool.exe"),
                PathBuf::from("C:\\parent\\tool.exe"),
                PathBuf::from("C:\\parent\\bin\\tool.exe"),
                PathBuf::from("C:\\parent\\tool.exe"),
                PathBuf::from("D:\\tools\\tool.exe"),
            ]
        );
        for program in ["tool.exe", "tool.com", "tool.cmd", "tool."] {
            assert_eq!(
                windows_candidates(
                    OsStr::new(program),
                    Some(OsStr::new("bin")),
                    Path::new("C:\\parent"),
                    vec![]
                )
                .unwrap(),
                [Path::new("C:\\parent\\bin").join(program)]
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_explicit_paths_do_not_search_or_use_child_cwd() {
        for (program, expected) in [
            ("bin\\tool.exe", "C:\\parent\\bin\\tool.exe"),
            ("D:\\tools\\tool.exe", "D:\\tools\\tool.exe"),
            ("\\\\server\\share\\tool.exe", "\\\\server\\share\\tool.exe"),
        ] {
            assert_eq!(
                windows_candidates(
                    OsStr::new(program),
                    Some(OsStr::new("C:\\other")),
                    Path::new("C:\\parent"),
                    vec![]
                )
                .unwrap(),
                [PathBuf::from(expected)]
            );
        }
        for program in ["C:tool.exe", "\\tool.exe", "\\\\?\\C:\\tool.exe"] {
            assert_eq!(
                windows_candidates(OsStr::new(program), None, Path::new("C:\\parent"), vec![])
                    .unwrap_err(),
                AsyncHostError::Inval
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_lookup_preserves_unpaired_utf16() {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let program = OsString::from_wide(&[b't' as u16, 0xd800]);
        let candidates = windows_candidates(
            &program,
            Some(OsStr::new("bin")),
            Path::new("C:\\parent"),
            vec![],
        )
        .unwrap();
        let expected: Vec<_> = OsStr::new("C:\\parent\\bin\\t")
            .encode_wide()
            .chain([0xd800])
            .chain(OsStr::new(".exe").encode_wide())
            .collect();
        assert_eq!(
            candidates[0].as_os_str().encode_wide().collect::<Vec<_>>(),
            expected
        );
    }
}
