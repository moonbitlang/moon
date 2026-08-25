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

//! Backend-neutral implementation of moonrun's permission-backed filesystem imports.
//!
//! Wasm runtime adapters convert guest values and expose imports. This module owns
//! filesystem authorization, OS operations, and the guest-visible error text.
//! WASI has its own descriptor and preopen capability model and does not pass
//! through this module.

pub(crate) mod v8;

mod job;

pub(crate) use job::Job;

#[cfg(test)]
pub(crate) use job::{STAT_OPEN_IDENTITY, compat_symbols};

use std::ffi::OsStr;
use std::fmt;
use std::path::Path;
use std::sync::Arc;

use crate::async_host::AsyncHostResult;
use crate::policy::Policy;
use crate::runtime::WorkingDirectory;

#[derive(Debug)]
pub(crate) struct HostFsError {
    message: String,
}

impl fmt::Display for HostFsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}

impl HostFsError {
    fn permission_denied(path: &str) -> Self {
        Self {
            message: format!("Permission denied: {path}"),
        }
    }

    fn operation(message: String) -> Self {
        Self { message }
    }
}

/// Results retained between a filesystem operation and its getter.
#[derive(Default)]
pub(crate) struct FsOperationResults {
    file_content: Vec<u8>,
    dir_files: Vec<String>,
    error_message: String,
}

impl FsOperationResults {
    pub(crate) fn file_content(&self) -> &[u8] {
        &self.file_content
    }

    pub(crate) fn dir_files(&self) -> &[String] {
        &self.dir_files
    }

    pub(crate) fn error_message(&self) -> &str {
        &self.error_message
    }
}

/// Engine-neutral filesystem operations that enforce the runtime policy
/// before accessing the operating system's filesystem.
pub(crate) struct HostFs {
    policy: Arc<Policy>,
    working_directory: WorkingDirectory,
}

impl HostFs {
    pub(crate) fn new(policy: Arc<Policy>, working_directory: WorkingDirectory) -> Self {
        Self {
            policy,
            working_directory,
        }
    }

    pub(crate) fn read_file_to_string(&self, path: &str) -> Result<String, HostFsError> {
        self.ensure_read(path)?;
        std::fs::read_to_string(path)
            .map_err(|_| HostFsError::operation(format!("Failed to read file: {path}")))
    }

    pub(crate) fn write_string_to_file(
        &self,
        path: &str,
        contents: &str,
    ) -> Result<(), HostFsError> {
        self.ensure_write(path)?;
        std::fs::write(path, contents)
            .map_err(|_| HostFsError::operation(format!("Failed to write file: {path}")))
    }

    pub(crate) fn write_bytes_to_file(
        &self,
        path: &str,
        contents: impl FnOnce() -> Vec<u8>,
    ) -> Result<(), HostFsError> {
        self.ensure_write(path)?;
        // Decode guest-owned contents only after authorization, matching the
        // existing import's observable failure order for untrusted guests.
        std::fs::write(path, contents())
            .map_err(|_| HostFsError::operation(format!("Failed to write file: {path}")))
    }

    pub(crate) fn create_dir(&self, path: &str) -> Result<(), HostFsError> {
        self.ensure_write(path)?;
        std::fs::create_dir_all(path)
            .map_err(|_| HostFsError::operation(format!("Failed to create directory: {path}")))
    }

    pub(crate) fn read_dir(&self, path: &str) -> Result<Vec<String>, HostFsError> {
        self.ensure_read(path)?;
        read_dir_entries(path)
            .map_err(|_| HostFsError::operation(format!("Failed to read directory: {path}")))
    }

    pub(crate) fn is_file(&self, path: &str) -> bool {
        self.ensure_read(path).is_ok() && Path::new(path).is_file()
    }

    pub(crate) fn is_dir(&self, path: &str) -> bool {
        self.ensure_read(path).is_ok() && Path::new(path).is_dir()
    }

    pub(crate) fn remove_file(&self, path: &str) -> Result<(), HostFsError> {
        self.ensure_remove(path)?;
        std::fs::remove_file(path)
            .map_err(|_| HostFsError::operation(format!("Failed to remove file: {path}")))
    }

    pub(crate) fn remove_dir(&self, path: &str) -> Result<(), HostFsError> {
        self.ensure_remove(path)?;
        std::fs::remove_dir_all(path)
            .map_err(|_| HostFsError::operation(format!("Failed to remove directory: {path}")))
    }

    pub(crate) fn path_exists(&self, path: &str) -> bool {
        self.ensure_read(path).is_ok() && Path::new(path).exists()
    }

    pub(crate) fn current_dir(&self) -> String {
        if self.ensure_read(".").is_err() {
            return String::new();
        }
        self.working_directory
            .current_dir()
            .unwrap_or_default()
            .to_str()
            .unwrap()
            .to_owned()
    }

    pub(crate) fn read_file_to_bytes_new(
        &self,
        results: &mut FsOperationResults,
        path: &str,
    ) -> i32 {
        let result = self.ensure_read(path).and_then(|()| {
            std::fs::read(path).map_err(|error| {
                HostFsError::operation(format!("Failed to read file {path}: {error}"))
            })
        });
        match result {
            Ok(contents) => {
                results.file_content = contents;
                0
            }
            Err(error) => set_error(results, error),
        }
    }

    pub(crate) fn write_bytes_to_file_new(
        &self,
        results: &mut FsOperationResults,
        path: &str,
        contents: impl FnOnce() -> Result<Vec<u8>, String>,
    ) -> i32 {
        // As above, the adapter supplies conversion lazily so denial wins over
        // an invalid guest byte-array value.
        let result = self
            .ensure_write(path)
            .and_then(|()| contents().map_err(HostFsError::operation))
            .and_then(|contents| {
                std::fs::write(path, contents).map_err(|error| {
                    HostFsError::operation(format!("Failed to write file {path}: {error}"))
                })
            });
        operation_status(results, result)
    }

    pub(crate) fn create_dir_new(&self, results: &mut FsOperationResults, path: &str) -> i32 {
        let result = self.ensure_write(path).and_then(|()| {
            std::fs::create_dir_all(path).map_err(|error| {
                HostFsError::operation(format!("Failed to create directory {path}: {error}"))
            })
        });
        operation_status(results, result)
    }

    pub(crate) fn read_dir_new(&self, results: &mut FsOperationResults, path: &str) -> i32 {
        let result = self.ensure_read(path).and_then(|()| {
            read_dir_entries(path).map_err(|error| {
                HostFsError::operation(format!("Failed to read directory {path}: {error}"))
            })
        });
        match result {
            Ok(files) => {
                results.dir_files = files;
                0
            }
            Err(error) => set_error(results, error),
        }
    }

    pub(crate) fn is_file_new(&self, results: &mut FsOperationResults, path: &str) -> i32 {
        self.metadata_kind(results, path, std::fs::Metadata::is_file)
    }

    pub(crate) fn is_dir_new(&self, results: &mut FsOperationResults, path: &str) -> i32 {
        self.metadata_kind(results, path, std::fs::Metadata::is_dir)
    }

    pub(crate) fn remove_file_new(&self, results: &mut FsOperationResults, path: &str) -> i32 {
        let result = self.ensure_remove(path).and_then(|()| {
            std::fs::remove_file(path).map_err(|error| {
                HostFsError::operation(format!("Failed to remove file {path}: {error}"))
            })
        });
        operation_status(results, result)
    }

    pub(crate) fn remove_dir_new(&self, results: &mut FsOperationResults, path: &str) -> i32 {
        let result = self.ensure_remove(path).and_then(|()| {
            std::fs::remove_dir_all(path).map_err(|error| {
                HostFsError::operation(format!("Failed to remove directory {path}: {error}"))
            })
        });
        operation_status(results, result)
    }

    fn metadata_kind(
        &self,
        results: &mut FsOperationResults,
        path: &str,
        kind: fn(&std::fs::Metadata) -> bool,
    ) -> i32 {
        let result = self.ensure_read(path).and_then(|()| {
            std::fs::metadata(path)
                .map(|metadata| i32::from(kind(&metadata)))
                .map_err(|error| HostFsError::operation(format!("{error}: {path}")))
        });
        match result {
            Ok(value) => value,
            Err(error) => set_error(results, error),
        }
    }

    fn ensure_read(&self, path: &str) -> Result<(), HostFsError> {
        ensure_read_policy(&self.policy, path).map_err(|_| HostFsError::permission_denied(path))
    }

    fn ensure_write(&self, path: &str) -> Result<(), HostFsError> {
        ensure_write_policy(&self.policy, path).map_err(|_| HostFsError::permission_denied(path))
    }

    fn ensure_remove(&self, path: &str) -> Result<(), HostFsError> {
        ensure_remove_policy(&self.policy, path).map_err(|_| HostFsError::permission_denied(path))
    }
}

fn ensure_read_policy(policy: &Policy, path: &str) -> AsyncHostResult<()> {
    policy.stat_path(OsStr::new(path))
}

fn ensure_write_policy(policy: &Policy, path: &str) -> AsyncHostResult<()> {
    policy.open_path(OsStr::new(path), 1, 1, false)
}

fn ensure_remove_policy(policy: &Policy, path: &str) -> AsyncHostResult<()> {
    policy.remove_path(OsStr::new(path))
}

fn read_dir_entries(path: &str) -> std::io::Result<Vec<String>> {
    Ok(std::fs::read_dir(path)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let entry_path = entry.path();
            let relative_path = entry_path.strip_prefix(path).unwrap();
            relative_path.to_str().map(ToOwned::to_owned)
        })
        .collect())
}

fn operation_status(results: &mut FsOperationResults, result: Result<(), HostFsError>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(error) => set_error(results, error),
    }
}

fn set_error(results: &mut FsOperationResults, error: HostFsError) -> i32 {
    results.error_message = error.to_string();
    -1
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn operation_results_belong_to_one_runtime() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("input.bin");
        std::fs::write(&path, [1, 2, 3]).unwrap();
        let host = HostFs::new(Arc::new(Policy::allow_all()), WorkingDirectory::Ambient);
        let mut first = FsOperationResults::default();
        let second = FsOperationResults::default();

        assert_eq!(
            host.read_file_to_bytes_new(&mut first, path.to_str().unwrap()),
            0
        );
        assert_eq!(first.file_content(), [1, 2, 3]);
        assert!(second.file_content().is_empty());
    }

    #[test]
    fn denied_status_operation_records_guest_error() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\n").unwrap();
        let host = HostFs::new(
            Arc::new(Policy::from_file(&policy_file).unwrap()),
            WorkingDirectory::Ambient,
        );
        let mut results = FsOperationResults::default();

        assert_eq!(host.read_file_to_bytes_new(&mut results, "denied.bin"), -1);
        assert_eq!(results.error_message(), "Permission denied: denied.bin");
    }

    #[test]
    fn authorization_precedes_guest_byte_conversion() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\n").unwrap();
        let host = HostFs::new(
            Arc::new(Policy::from_file(&policy_file).unwrap()),
            WorkingDirectory::Ambient,
        );
        let mut results = FsOperationResults::default();
        let converted = Cell::new(false);

        let status = host.write_bytes_to_file_new(&mut results, "denied.bin", || {
            converted.set(true);
            Ok(Vec::new())
        });

        assert_eq!(status, -1);
        assert!(!converted.get());
        assert_eq!(results.error_message(), "Permission denied: denied.bin");
    }

    #[cfg(unix)]
    #[test]
    fn remove_policy_checks_link_path_not_target() {
        use crate::async_host::AsyncHostError;

        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&denied).unwrap();
        let allowed_file = allowed.join("target.txt");
        let denied_link = denied.join("link.txt");
        std::fs::write(&allowed_file, "target").unwrap();
        std::os::unix::fs::symlink(&allowed_file, &denied_link).unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nwrite = [\"allowed\"]\n").unwrap();
        let policy = Policy::from_file(&policy_file).unwrap();

        assert_eq!(
            ensure_remove_policy(&policy, denied_link.to_str().unwrap()),
            Err(AsyncHostError::PermissionDenied)
        );
    }
}
