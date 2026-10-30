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

//! Pure executable candidate generation from explicit lookup inputs.
//!
//! This module neither observes ambient state nor probes candidate files.
//! Candidate order and path spelling are retained: the executing platform
//! decides which failures permit trying another image. These paths are names,
//! not canonicalized file identities or authorization decisions.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum LookupError {
    #[error("the executable name is empty")]
    EmptyProgram,
    #[error("invalid executable lookup inputs")]
    InvalidInput,
}

#[derive(Debug)]
pub(super) struct LookupPlan {
    candidates: Vec<PathBuf>,
    #[cfg(any(unix, test))]
    search: bool,
}

impl LookupPlan {
    pub(super) fn candidates(&self) -> &[PathBuf] {
        &self.candidates
    }

    /// Whether execution still needs a caller-selected directory to interpret
    /// relative candidates. Absolute-only lookup does not need getcwd.
    pub(super) fn needs_cwd(&self) -> bool {
        self.candidates.iter().any(|path| !path.is_absolute())
    }

    /// Complete the same plan after its owner captures the required directory.
    /// Unix uses the child's directory; Windows uses the caller's directory.
    pub(super) fn resolve_from(mut self, cwd: &Path) -> Result<Self, LookupError> {
        if !cwd.is_absolute() || cwd.as_os_str().as_encoded_bytes().contains(&0) {
            return Err(LookupError::InvalidInput);
        }
        for candidate in &mut self.candidates {
            if !candidate.is_absolute() {
                *candidate = cwd.join(&*candidate);
            }
        }
        Ok(self)
    }

    /// Whether this request searches directories rather than naming one path.
    /// Execution uses this distinction for platform error and script fallback.
    #[cfg(any(unix, test))]
    pub(super) fn is_search(&self) -> bool {
        self.search
    }
}

/// Generate candidates from explicit PATH and optional captured child cwd.
/// `Some` requires an absolute cwd and freezes absolute candidates for owned
/// state. `None` preserves native relative paths for execution after child
/// chdir, without requiring the ambient directory to have a name. Empty PATH
/// entries denote that directory; missing PATH uses the native spawn default.
/// Neither tilde nor shell variables are expanded.
#[cfg(unix)]
pub(super) fn unix(
    program: &OsStr,
    path: Option<&OsStr>,
    child_cwd: Option<&Path>,
) -> Result<LookupPlan, LookupError> {
    use std::os::unix::ffi::OsStrExt;

    if program.is_empty() {
        return Err(LookupError::EmptyProgram);
    }
    if child_cwd.is_some_and(|cwd| !cwd.is_absolute() || cwd.as_os_str().as_bytes().contains(&0))
        || program.as_bytes().contains(&0)
        || path.is_some_and(|path| path.as_bytes().contains(&0))
    {
        return Err(LookupError::InvalidInput);
    }
    let search = !program.as_bytes().contains(&b'/');
    let candidates = if search {
        // Darwin _PATH_DEFPATH, musl's execvp default, and glibc CS_PATH.
        // These are spawn defaults, not the shell's login PATH.
        #[cfg(target_os = "macos")]
        let default_path = OsStr::new("/usr/bin:/bin");
        #[cfg(all(target_os = "linux", target_env = "musl"))]
        let default_path = OsStr::new("/usr/local/bin:/bin:/usr/bin");
        #[cfg(not(any(target_os = "macos", all(target_os = "linux", target_env = "musl"))))]
        let default_path = OsStr::new("/bin:/usr/bin");

        path.unwrap_or(default_path)
            .as_bytes()
            .split(|byte| *byte == b':')
            .map(|directory| {
                let candidate = Path::new(OsStr::from_bytes(directory)).join(program);
                match child_cwd {
                    Some(cwd) => cwd.join(candidate),
                    None => candidate,
                }
            })
            .collect()
    } else {
        vec![child_cwd.map_or_else(|| PathBuf::from(program), |cwd| cwd.join(program))]
    };
    Ok(LookupPlan { candidates, search })
}

/// Interpret an already parsed Windows executable token. The caller supplies
/// search directories in their intended order, including "." or the caller
/// cwd only when enabled. PATH entries follow those directories. Without a
/// captured caller cwd, relative candidates remain explicit in the plan so its
/// owner can observe cwd only if needed, independently of the child's cwd.
#[cfg(windows)]
pub(super) fn windows(
    program: &OsStr,
    path: Option<&OsStr>,
    caller_cwd: Option<&Path>,
    mut search_directories: Vec<PathBuf>,
) -> Result<LookupPlan, LookupError> {
    use std::os::windows::ffi::OsStrExt;

    if program.is_empty() {
        return Err(LookupError::EmptyProgram);
    }
    if caller_cwd.is_some_and(|cwd| {
        !cwd.is_absolute() || cwd.as_os_str().encode_wide().any(|unit| unit == 0)
    }) || path.is_some_and(|path| path.encode_wide().any(|unit| unit == 0))
    {
        return Err(LookupError::InvalidInput);
    }
    let program_path = Path::new(program);
    validate_windows_path(program_path)?;
    if program_path.is_absolute() {
        return Ok(LookupPlan {
            candidates: vec![program_path.to_owned()],
            #[cfg(test)]
            search: false,
        });
    }
    if program
        .encode_wide()
        .any(|unit| unit == b'\\' as u16 || unit == b'/' as u16)
    {
        return Ok(LookupPlan {
            candidates: vec![
                caller_cwd.map_or_else(|| program_path.to_owned(), |cwd| cwd.join(program_path)),
            ],
            #[cfg(test)]
            search: false,
        });
    }

    // CreateProcess lookup is not cmd.exe/PATHEXT lookup. Only an
    // extensionless bare name gets .exe; a trailing dot suppresses it.
    let mut filename = program.to_owned();
    if program_path.extension().is_none() && program.as_encoded_bytes().last() != Some(&b'.') {
        filename.push(".exe");
    }
    for directory in &search_directories {
        if directory.as_os_str().encode_wide().any(|unit| unit == 0) {
            return Err(LookupError::InvalidInput);
        }
        if !directory.is_absolute() {
            validate_windows_path(directory)?;
        }
    }
    if let Some(path) = path {
        // split_paths only parses the supplied value; it does not read Env.
        for directory in std::env::split_paths(path) {
            validate_windows_path(&directory)?;
            search_directories.push(directory);
        }
    }
    Ok(LookupPlan {
        candidates: search_directories
            .into_iter()
            .map(|directory| {
                let directory = match caller_cwd {
                    Some(cwd) => cwd.join(directory),
                    None => directory,
                };
                directory.join(&filename)
            })
            .collect(),
        #[cfg(test)]
        search: true,
    })
}

#[cfg(windows)]
fn validate_windows_path(path: &Path) -> Result<(), LookupError> {
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Component, Prefix};

    if path.as_os_str().encode_wide().any(|unit| unit == 0) {
        return Err(LookupError::InvalidInput);
    }
    // Drive-relative and rooted-without-drive paths need an explicit per-drive
    // directory model. Device paths have separate normalization rules. Keep
    // these unsupported in interpreted lookup rather than consulting hidden
    // host drive state. Ordinary fully qualified drive and UNC paths work.
    match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(_) if path.is_absolute() => Ok(()),
            Prefix::UNC(_, _) => Ok(()),
            _ => Err(LookupError::InvalidInput),
        },
        Some(Component::RootDir) => Err(LookupError::InvalidInput),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn unix_observes_cwd_only_for_relative_candidates() {
        for (program, path) in [("/bin/tool", None), ("tool", Some("/bin:/usr/bin"))] {
            let plan = unix(OsStr::new(program), path.map(OsStr::new), None).unwrap();
            assert!(!plan.needs_cwd());
        }
        let plan = unix(OsStr::new("tool"), Some(OsStr::new("bin::/usr/bin")), None).unwrap();
        assert!(plan.needs_cwd());
        let plan = plan.resolve_from(Path::new("/runtime/child")).unwrap();
        assert!(!plan.needs_cwd());
        assert_eq!(
            plan.candidates(),
            [
                PathBuf::from("/runtime/child/bin/tool"),
                PathBuf::from("/runtime/child/tool"),
                PathBuf::from("/usr/bin/tool"),
            ]
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_observes_caller_cwd_only_for_relative_candidates() {
        let absolute = windows(OsStr::new("C:\\bin\\tool.exe"), None, None, vec![]).unwrap();
        assert!(!absolute.needs_cwd());
        let absolute_search = windows(
            OsStr::new("tool"),
            Some(OsStr::new("D:\\bin")),
            None,
            vec![PathBuf::from("C:\\system")],
        )
        .unwrap();
        assert!(!absolute_search.needs_cwd());
        let relative = windows(
            OsStr::new("tool"),
            Some(OsStr::new("bin;D:\\bin")),
            None,
            vec![PathBuf::from("C:\\application"), PathBuf::from(".")],
        )
        .unwrap();
        assert!(relative.needs_cwd());
        let resolved = relative.resolve_from(Path::new("C:\\runtime")).unwrap();
        assert!(!resolved.needs_cwd());
        assert_eq!(
            resolved.candidates(),
            [
                PathBuf::from("C:\\application\\tool.exe"),
                PathBuf::from("C:\\runtime\\.\\tool.exe"),
                PathBuf::from("C:\\runtime\\bin\\tool.exe"),
                PathBuf::from("D:\\bin\\tool.exe"),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_native_candidates_do_not_need_a_captured_directory() {
        let plan = unix(OsStr::new("tool"), Some(OsStr::new("../bin::/bin")), None).unwrap();
        assert_eq!(
            plan.candidates(),
            [
                PathBuf::from("../bin/tool"),
                PathBuf::from("tool"),
                PathBuf::from("/bin/tool")
            ]
        );
        for program in ["./tool", "/bin/tool"] {
            let plan = unix(OsStr::new(program), None, None).unwrap();
            assert!(!plan.is_search());
            assert_eq!(plan.candidates(), [PathBuf::from(program)]);
        }
    }

    #[cfg(unix)]
    #[test]
    fn unix_relative_and_empty_entries_preserve_order_without_probing() {
        let plan = unix(
            OsStr::new("does-not-have-to-exist"),
            Some(OsStr::new("relative::/absolute:~/literal:relative:")),
            Some(Path::new("/child")),
        )
        .unwrap();
        assert!(plan.is_search());
        assert_eq!(
            plan.candidates(),
            [
                PathBuf::from("/child/relative/does-not-have-to-exist"),
                PathBuf::from("/child/does-not-have-to-exist"),
                PathBuf::from("/absolute/does-not-have-to-exist"),
                PathBuf::from("/child/~/literal/does-not-have-to-exist"),
                PathBuf::from("/child/relative/does-not-have-to-exist"),
                PathBuf::from("/child/does-not-have-to-exist"),
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
            let plan = unix(
                OsStr::new(program),
                Some(OsStr::new("/other")),
                Some(Path::new("/child")),
            )
            .unwrap();
            assert!(!plan.is_search());
            assert_eq!(plan.candidates(), [PathBuf::from(expected)]);
        }
    }

    #[cfg(unix)]
    #[test]
    fn unix_empty_path_searches_cwd_but_missing_path_uses_platform_defaults() {
        let empty = unix(
            OsStr::new("tool"),
            Some(OsStr::new("")),
            Some(Path::new("/child")),
        )
        .unwrap();
        assert_eq!(empty.candidates(), [PathBuf::from("/child/tool")]);
        let missing = unix(OsStr::new("tool"), None, Some(Path::new("/child"))).unwrap();
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
    fn unix_native_bytes_are_preserved_and_invalid_inputs_rejected() {
        use std::os::unix::ffi::OsStrExt;

        let plan = unix(
            OsStr::from_bytes(b"tool\xff"),
            Some(OsStr::from_bytes(b"bin\xfe")),
            Some(Path::new("/child")),
        )
        .unwrap();
        assert_eq!(
            plan.candidates()[0].as_os_str().as_bytes(),
            b"/child/bin\xfe/tool\xff"
        );
        assert_eq!(
            unix(OsStr::new(""), None, Some(Path::new("/child"))).unwrap_err(),
            LookupError::EmptyProgram
        );
        for (program, path, cwd) in [
            ("tool\0bad", None, "/child"),
            ("tool", Some("bin\0bad"), "/child"),
            ("tool", None, "/child\0bad"),
            ("tool", None, "relative"),
        ] {
            assert_eq!(
                unix(
                    OsStr::new(program),
                    path.map(OsStr::new),
                    Some(Path::new(cwd))
                )
                .unwrap_err(),
                LookupError::InvalidInput
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_native_directories_precede_parent_path_without_pathext() {
        let plan = windows(
            OsStr::new("tool"),
            Some(OsStr::new("bin;;D:\\tools")),
            Some(Path::new("C:\\parent")),
            vec![
                PathBuf::from("C:\\application"),
                PathBuf::from("C:\\parent"),
            ],
        )
        .unwrap();
        assert!(plan.is_search());
        assert_eq!(
            plan.candidates(),
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
                windows(
                    OsStr::new(program),
                    Some(OsStr::new("bin")),
                    Some(Path::new("C:\\parent")),
                    vec![]
                )
                .unwrap()
                .candidates(),
                [Path::new("C:\\parent\\bin").join(program)]
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_explicit_paths_use_only_the_parent_directory() {
        for (program, expected) in [
            ("bin\\tool.exe", "C:\\parent\\bin\\tool.exe"),
            ("bin/tool.exe", "C:\\parent\\bin/tool.exe"),
            ("D:\\tools\\tool.exe", "D:\\tools\\tool.exe"),
            ("\\\\server\\share\\tool.exe", "\\\\server\\share\\tool.exe"),
        ] {
            let plan = windows(
                OsStr::new(program),
                Some(OsStr::new("C:\\other")),
                Some(Path::new("C:\\parent")),
                vec![PathBuf::from("C:\\native")],
            )
            .unwrap();
            assert!(!plan.is_search());
            assert_eq!(plan.candidates(), [PathBuf::from(expected)]);
        }
        for program in ["C:tool.exe", "\\tool.exe", "\\\\?\\C:\\tool.exe"] {
            assert_eq!(
                windows(
                    OsStr::new(program),
                    None,
                    Some(Path::new("C:\\parent")),
                    vec![]
                )
                .unwrap_err(),
                LookupError::InvalidInput
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_has_no_implicit_search_directories() {
        let missing = windows(
            OsStr::new("tool"),
            None,
            Some(Path::new("C:\\parent")),
            vec![],
        )
        .unwrap();
        assert!(missing.is_search());
        assert!(missing.candidates().is_empty());
        let empty = windows(
            OsStr::new("tool"),
            Some(OsStr::new("")),
            Some(Path::new("C:\\parent")),
            vec![],
        )
        .unwrap();
        assert_eq!(empty.candidates(), [PathBuf::from("C:\\parent\\tool.exe")]);
    }

    #[cfg(windows)]
    #[test]
    fn windows_rejects_invalid_inputs_and_implicit_drive_state() {
        assert_eq!(
            windows(OsStr::new(""), None, Some(Path::new("C:\\parent")), vec![]).unwrap_err(),
            LookupError::EmptyProgram
        );
        for (program, path, cwd, directories) in [
            ("tool\0bad", None, "C:\\parent", vec![]),
            ("tool", Some("bin\0bad"), "C:\\parent", vec![]),
            ("tool", None, "C:\\parent\0bad", vec![]),
            ("tool", None, "relative", vec![]),
            (
                "tool",
                None,
                "C:\\parent",
                vec![PathBuf::from("C:relative")],
            ),
            (
                "tool",
                None,
                "C:\\parent",
                vec![PathBuf::from("C:\\native\0bad")],
            ),
            ("tool", Some("C:bin"), "C:\\parent", vec![]),
            ("tool", Some("\\bin"), "C:\\parent", vec![]),
        ] {
            assert_eq!(
                windows(
                    OsStr::new(program),
                    path.map(OsStr::new),
                    Some(Path::new(cwd)),
                    directories
                )
                .unwrap_err(),
                LookupError::InvalidInput
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_preserves_unpaired_utf16_and_quoted_path_entries() {
        use std::ffi::OsString;
        use std::os::windows::ffi::{OsStrExt, OsStringExt};

        let program = OsString::from_wide(&[b't' as u16, 0xd800]);
        let plan = windows(
            &program,
            Some(OsStr::new("\"bin with space\"")),
            Some(Path::new("C:\\parent")),
            vec![],
        )
        .unwrap();
        let expected: Vec<_> = OsStr::new("C:\\parent\\bin with space\\t")
            .encode_wide()
            .chain([0xd800])
            .chain(OsStr::new(".exe").encode_wide())
            .collect();
        assert_eq!(
            plan.candidates()[0]
                .as_os_str()
                .encode_wide()
                .collect::<Vec<_>>(),
            expected
        );
    }
}
