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

//! Path resolution used by filesystem authorization.

use std::io;
use std::path::{Component, Path, PathBuf};

use super::host::FsError;

pub(super) fn directory_path(handle: super::os::RawFile) -> Result<PathBuf, FsError> {
    // A directory reported as missing cannot receive a fresh pathname grant.
    super::os::directory::path(handle).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            FsError::Denied
        } else {
            FsError::Io(error)
        }
    })
}

pub(super) fn canonicalize_existing_prefix(path: &Path) -> Result<PathBuf, FsError> {
    for ancestor in path.ancestors() {
        match std::fs::canonicalize(ancestor) {
            Ok(prefix) => {
                let suffix = path.strip_prefix(ancestor).map_err(|_| FsError::Denied)?;
                // Lexically cancelling a missing component with `..` could
                // hide later links that recursive creation will traverse.
                if suffix.components().any(|part| part == Component::ParentDir) {
                    return Err(FsError::Denied);
                }
                return Ok(normalize_path(prefix.join(suffix)));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(FsError::Denied),
        }
        // Only missing entries may use prefix fallback. An unresolved existing
        // link can redirect creation outside the authorized prefix. Strip a
        // directory suffix so lstat inspects the entry rather than following it.
        let entry: PathBuf = ancestor.components().collect();
        match std::fs::symlink_metadata(entry) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            _ => return Err(FsError::Denied),
        }
    }
    Err(FsError::Denied)
}

/// A Unix directory suffix makes the named component participate in directory
/// traversal, even for an operation normally acting on the final link entry.
/// Inspect the original bytes: Path::file_name deliberately ignores this suffix.
/// The returned path is only for inspection; native operations retain `path`.
pub(crate) fn entry_before_directory_suffix(path: &Path) -> Option<PathBuf> {
    let bytes = path.as_os_str().as_encoded_bytes();
    if cfg!(unix) && (bytes.ends_with(b"/") || bytes.ends_with(b"/.")) {
        Some(path.parent()?.join(path.file_name()?))
    } else {
        None
    }
}

pub(super) fn canonicalize_entry_path(path: &Path) -> Result<PathBuf, FsError> {
    if let Some(entry) = entry_before_directory_suffix(path) {
        if let Ok(resolved) = std::fs::canonicalize(path) {
            return Ok(resolved);
        }
        // A dangling link is not a missing directory entry. Prefix fallback
        // would authorize the link's name while mkdir/rename may create its target.
        if std::fs::symlink_metadata(entry).is_ok_and(|metadata| metadata.is_symlink()) {
            return Err(FsError::Denied);
        }
        return canonicalize_existing_prefix(path);
    }
    let parent = path.parent().ok_or(FsError::Denied)?;
    let file_name = path.file_name().ok_or(FsError::Denied)?;
    Ok(normalize_path(
        canonicalize_existing_prefix(parent)?.join(file_name),
    ))
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}
