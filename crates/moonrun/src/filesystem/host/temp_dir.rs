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

//! Temporary-directory selection from Runtime Env, preserving native defaults.

use super::FsError;
use crate::async_sys::ported_fns;
use crate::runtime::Env;
use std::ffi::OsString;

#[cfg(all(unix, target_os = "android"))]
const DEFAULT_TEMP_DIR: &str = "/data/local/tmp/";
#[cfg(all(unix, not(target_os = "android")))]
const DEFAULT_TEMP_DIR: &str = "/tmp/";

ported_fns! {
    #[ported(source = "src/fs/stub.c", original = "moonbitlang_async_get_tmp_path")]
    pub(super) fn resolve(environment: &Env) -> Result<OsString, FsError> {
        #[cfg(unix)]
        {
            // Env owns both ambient lookup and per-Runtime overrides.
            // Async appends a generated filename directly to this base path.
            Ok(environment.get("TMPDIR".as_ref())
                .and_then(with_separator)
                .unwrap_or_else(|| OsString::from(DEFAULT_TEMP_DIR)))
        }
        #[cfg(windows)]
        {
            // Native Windows has fallbacks beyond TMP and TEMP. Owned Env must
            // not reach those process-wide inputs when neither variable is set.
            if environment.is_ambient() {
                return Ok(crate::filesystem::os::path::temp_dir()?);
            }
            environment.get("TMP".as_ref()).and_then(with_separator)
                .or_else(|| environment.get("TEMP".as_ref()).and_then(with_separator))
                .ok_or(FsError::Denied)
        }
    }
}

#[cfg(unix)]
fn with_separator(path: OsString) -> Option<OsString> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let bytes = path.as_os_str().as_bytes();
    if bytes.is_empty() {
        return None;
    }
    if bytes.ends_with(b"/") {
        return Some(path);
    }
    let mut bytes = path.into_vec();
    bytes.push(b'/');
    Some(OsString::from_vec(bytes))
}

#[cfg(windows)]
fn with_separator(path: OsString) -> Option<OsString> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let mut units = path.encode_wide().collect::<Vec<_>>();
    if units.is_empty() {
        return None;
    }
    if !matches!(units.last(), Some(unit) if *unit == b'\\' as u16 || *unit == b'/' as u16) {
        units.push(b'\\' as u16);
    }
    Some(OsString::from_wide(&units))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambient_path_is_non_empty_and_separator_terminated() {
        let path = resolve(&Env::ambient()).unwrap();
        assert!(!path.is_empty());
        let path = path.to_string_lossy();
        assert!(path.ends_with('/') || path.ends_with('\\'));
    }

    #[test]
    fn normalization_preserves_existing_separators_and_ignores_empty_values() {
        for path in ["/tmp/", "relative/"] {
            assert_eq!(
                with_separator(OsString::from(path)),
                Some(OsString::from(path))
            );
        }
        assert_eq!(with_separator(OsString::new()), None);
    }
}
