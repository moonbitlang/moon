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

//! Unstable filesystem ABI presentation: legacy strings, status codes and getter state.
//! Filesystem authorization and effects stay in HostFs.

use crate::filesystem::{FsError, HostFs};
use std::fmt;
use std::path::Path;

#[derive(Debug)]
pub(super) struct UnstableFsError {
    message: String,
}

impl fmt::Display for UnstableFsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}

impl std::error::Error for UnstableFsError {}

impl UnstableFsError {
    fn from_fs(
        error: FsError,
        path: &str,
        io_message: impl FnOnce(std::io::Error) -> String,
    ) -> Self {
        match error {
            FsError::Denied => Self::permission_denied(path),
            FsError::Io(error) => Self::operation(io_message(error)),
        }
    }

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
pub(super) struct FsOperationResults {
    file_content: Vec<u8>,
    dir_files: Vec<String>,
    error_message: String,
}

impl FsOperationResults {
    pub(super) fn file_content(&self) -> &[u8] {
        &self.file_content
    }

    pub(super) fn dir_files(&self) -> &[String] {
        &self.dir_files
    }

    pub(super) fn error_message(&self) -> &str {
        &self.error_message
    }
}

/// Borrowed presentation adapter; it owns no filesystem state or authority.
pub(super) struct UnstableFs<'a>(pub(super) &'a HostFs);

// Preserve payload errors verbatim while formatting filesystem errors for the
// particular unstable import. HostFs still controls authorization-before-decoding.
enum ContentError<E> {
    Fs(FsError),
    Contents(E),
}
impl<E> From<FsError> for ContentError<E> {
    fn from(error: FsError) -> Self {
        Self::Fs(error)
    }
}

impl UnstableFs<'_> {
    pub(super) fn read_file_to_string(&self, path: &str) -> Result<String, UnstableFsError> {
        self.0.read_text(Path::new(path)).map_err(|error| {
            UnstableFsError::from_fs(error, path, |_| format!("Failed to read file: {path}"))
        })
    }

    pub(super) fn write_string_to_file(
        &self,
        path: &str,
        contents: &str,
    ) -> Result<(), UnstableFsError> {
        self.0
            .write_file(Path::new(path), contents.as_bytes())
            .map_err(|error| {
                UnstableFsError::from_fs(error, path, |_| format!("Failed to write file: {path}"))
            })
    }

    pub(super) fn write_bytes_to_file<E>(
        &self,
        path: &str,
        contents: impl FnOnce() -> Result<Vec<u8>, E>,
    ) -> Result<(), E>
    where
        E: From<UnstableFsError>,
    {
        match self.0.write_file_with(Path::new(path), || {
            contents().map_err(ContentError::Contents)
        }) {
            Ok(()) => Ok(()),
            Err(ContentError::Fs(error)) => Err(UnstableFsError::from_fs(error, path, |_| {
                format!("Failed to write file: {path}")
            })
            .into()),
            Err(ContentError::Contents(error)) => Err(error),
        }
    }

    pub(super) fn create_dir(&self, path: &str) -> Result<(), UnstableFsError> {
        self.0
            .create_directory_all(Path::new(path))
            .map_err(|error| {
                UnstableFsError::from_fs(error, path, |_| {
                    format!("Failed to create directory: {path}")
                })
            })
    }

    pub(super) fn read_dir(&self, path: &str) -> Result<Vec<String>, UnstableFsError> {
        self.0
            .read_directory(Path::new(path))
            .map(read_dir_entries)
            .map_err(|error| match error {
                FsError::Denied => UnstableFsError::permission_denied(path),
                FsError::Io(_) => {
                    UnstableFsError::operation(format!("Failed to read directory: {path}"))
                }
            })
    }

    pub(super) fn is_file(&self, path: &str) -> bool {
        self.0
            .metadata(Path::new(path))
            .is_ok_and(|metadata| metadata.is_file())
    }

    pub(super) fn is_dir(&self, path: &str) -> bool {
        self.0
            .metadata(Path::new(path))
            .is_ok_and(|metadata| metadata.is_dir())
    }

    pub(super) fn remove_file(&self, path: &str) -> Result<(), UnstableFsError> {
        self.0
            .unlink_file(Path::new(path))
            .map_err(|error| match error {
                FsError::Denied => UnstableFsError::permission_denied(path),
                FsError::Io(_) => {
                    UnstableFsError::operation(format!("Failed to remove file: {path}"))
                }
            })
    }

    pub(super) fn remove_dir(&self, path: &str) -> Result<(), UnstableFsError> {
        self.0
            .remove_directory_all(Path::new(path))
            .map_err(|error| {
                UnstableFsError::from_fs(error, path, |_| {
                    format!("Failed to remove directory: {path}")
                })
            })
    }

    pub(super) fn path_exists(&self, path: &str) -> bool {
        self.0.metadata(Path::new(path)).is_ok()
    }

    pub(super) fn current_dir(&self) -> String {
        self.0
            .current_directory()
            .unwrap_or_default()
            .to_str()
            .unwrap()
            .to_owned()
    }

    pub(super) fn read_file_to_bytes_new(
        &self,
        results: &mut FsOperationResults,
        path: &str,
    ) -> i32 {
        let result = self.0.read_file(Path::new(path)).map_err(|error| {
            UnstableFsError::from_fs(error, path, |error| {
                format!("Failed to read file {path}: {error}")
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

    pub(super) fn write_bytes_to_file_new(
        &self,
        results: &mut FsOperationResults,
        path: &str,
        contents: impl FnOnce() -> Result<Vec<u8>, String>,
    ) -> i32 {
        let result = match self.0.write_file_with(Path::new(path), || {
            contents().map_err(ContentError::Contents)
        }) {
            Ok(()) => Ok(()),
            Err(ContentError::Fs(error)) => Err(UnstableFsError::from_fs(error, path, |error| {
                format!("Failed to write file {path}: {error}")
            })),
            Err(ContentError::Contents(error)) => Err(UnstableFsError::operation(error)),
        };
        operation_status(results, result)
    }

    pub(super) fn create_dir_new(&self, results: &mut FsOperationResults, path: &str) -> i32 {
        let result = self
            .0
            .create_directory_all(Path::new(path))
            .map_err(|error| {
                UnstableFsError::from_fs(error, path, |error| {
                    format!("Failed to create directory {path}: {error}")
                })
            });
        operation_status(results, result)
    }

    pub(super) fn read_dir_new(&self, results: &mut FsOperationResults, path: &str) -> i32 {
        let result = self
            .0
            .read_directory(Path::new(path))
            .map(read_dir_entries)
            .map_err(|error| match error {
                FsError::Denied => UnstableFsError::permission_denied(path),
                FsError::Io(error) => {
                    UnstableFsError::operation(format!("Failed to read directory {path}: {error}"))
                }
            });
        match result {
            Ok(files) => {
                results.dir_files = files;
                0
            }
            Err(error) => set_error(results, error),
        }
    }

    pub(super) fn is_file_new(&self, results: &mut FsOperationResults, path: &str) -> i32 {
        self.metadata_kind(results, path, std::fs::Metadata::is_file)
    }

    pub(super) fn is_dir_new(&self, results: &mut FsOperationResults, path: &str) -> i32 {
        self.metadata_kind(results, path, std::fs::Metadata::is_dir)
    }

    pub(super) fn remove_file_new(&self, results: &mut FsOperationResults, path: &str) -> i32 {
        let result = self
            .0
            .unlink_file(Path::new(path))
            .map_err(|error| match error {
                FsError::Denied => UnstableFsError::permission_denied(path),
                FsError::Io(error) => {
                    UnstableFsError::operation(format!("Failed to remove file {path}: {error}"))
                }
            });
        operation_status(results, result)
    }

    pub(super) fn remove_dir_new(&self, results: &mut FsOperationResults, path: &str) -> i32 {
        let result = self
            .0
            .remove_directory_all(Path::new(path))
            .map_err(|error| {
                UnstableFsError::from_fs(error, path, |error| {
                    format!("Failed to remove directory {path}: {error}")
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
        let result = self
            .0
            .metadata(Path::new(path))
            .map(|metadata| i32::from(kind(&metadata)))
            .map_err(|error| {
                UnstableFsError::from_fs(error, path, |error| format!("{error}: {path}"))
            });
        match result {
            Ok(value) => value,
            Err(error) => set_error(results, error),
        }
    }
}

fn read_dir_entries(entries: std::fs::ReadDir) -> Vec<String> {
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str().map(ToOwned::to_owned))
        .collect()
}

fn operation_status(results: &mut FsOperationResults, result: Result<(), UnstableFsError>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(error) => set_error(results, error),
    }
}

fn set_error(results: &mut FsOperationResults, error: UnstableFsError) -> i32 {
    results.error_message = error.to_string();
    -1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{self, Policy};
    use crate::runtime::{Env, WorkingDirectory};
    use std::{cell::Cell, sync::Arc};
    #[cfg(unix)]
    use std::{ffi::OsStr, path::PathBuf};

    fn host_fs(mut policy: Policy, environment: Arc<Env>) -> HostFs {
        HostFs::new(
            policy.take_filesystem_policy(),
            environment,
            Arc::new(WorkingDirectory::Ambient),
        )
    }

    #[cfg(unix)]
    #[test]
    fn directory_enumeration_does_not_substitute_lossy_path() {
        use std::os::unix::ffi::OsStrExt;

        let tmp = tempfile::tempdir().unwrap();
        let directory = tmp.path().join(OsStr::from_bytes(b"workspace-\xff"));
        let lossy_directory = PathBuf::from(directory.to_string_lossy().into_owned());
        std::fs::create_dir(&lossy_directory).unwrap();
        std::fs::write(lossy_directory.join("wrong.txt"), []).unwrap();
        let host = HostFs::new(
            None,
            Arc::new(Env::ambient()),
            Arc::new(WorkingDirectory::Fixed(directory)),
        );

        let legacy_result = UnstableFs(&host).read_dir(".");
        let mut results = FsOperationResults::default();
        assert_eq!(UnstableFs(&host).read_dir_new(&mut results, "."), -1);
        assert!(legacy_result.is_err());
        assert!(results.dir_files().is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn directory_enumeration_preserves_non_utf8_working_directory() {
        use std::os::unix::ffi::OsStrExt;

        let tmp = tempfile::tempdir().unwrap();
        let directory = tmp.path().join(OsStr::from_bytes(b"workspace-\xff"));
        let lossy_directory = PathBuf::from(directory.to_string_lossy().into_owned());
        std::fs::create_dir(&directory).unwrap();
        std::fs::create_dir(&lossy_directory).unwrap();
        std::fs::write(directory.join("expected.txt"), []).unwrap();
        std::fs::write(lossy_directory.join("wrong.txt"), []).unwrap();
        let host = HostFs::new(
            None,
            Arc::new(Env::ambient()),
            Arc::new(WorkingDirectory::Fixed(directory)),
        );

        let legacy_entries = UnstableFs(&host).read_dir(".").unwrap();
        let mut results = FsOperationResults::default();
        assert_eq!(UnstableFs(&host).read_dir_new(&mut results, "."), 0);
        let expected = ["expected.txt".to_owned()];
        assert_eq!(
            (legacy_entries.as_slice(), results.dir_files()),
            (expected.as_slice(), expected.as_slice())
        );
    }

    #[test]
    fn operation_results_belong_to_one_runtime() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("input.bin");
        std::fs::write(&path, [1, 2, 3]).unwrap();
        let host = host_fs(Policy::allow_all(), Arc::new(Env::ambient()));
        let mut first = FsOperationResults::default();
        let second = FsOperationResults::default();

        assert_eq!(
            UnstableFs(&host).read_file_to_bytes_new(&mut first, path.to_str().unwrap()),
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
        let (policy, _) = policy::load_file(&policy_file).unwrap();
        let host = host_fs(policy, Arc::new(Env::ambient()));
        let mut results = FsOperationResults::default();

        assert_eq!(
            UnstableFs(&host).read_file_to_bytes_new(&mut results, "denied.bin"),
            -1
        );
        assert_eq!(results.error_message(), "Permission denied: denied.bin");
    }

    #[test]
    fn authorization_precedes_guest_byte_conversion() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\n").unwrap();
        let (policy, _) = policy::load_file(&policy_file).unwrap();
        let host = host_fs(policy, Arc::new(Env::ambient()));
        let mut results = FsOperationResults::default();
        let converted = Cell::new(false);

        let error = UnstableFs(&host)
            .write_bytes_to_file("denied.bin", || {
                converted.set(true);
                Ok::<_, UnstableFsError>(Vec::new())
            })
            .unwrap_err();

        assert!(!converted.get());
        assert_eq!(error.to_string(), "Permission denied: denied.bin");

        let status = UnstableFs(&host).write_bytes_to_file_new(&mut results, "denied.bin", || {
            converted.set(true);
            Ok(Vec::new())
        });

        assert_eq!(status, -1);
        assert!(!converted.get());
        assert_eq!(results.error_message(), "Permission denied: denied.bin");
    }
    #[test]
    fn failed_payload_conversion_neither_creates_nor_truncates_files() {
        let directory = tempfile::tempdir().unwrap();
        let host = host_fs(Policy::allow_all(), Arc::new(Env::ambient()));
        let unstable = UnstableFs(&host);
        for existing in [false, true] {
            let path = directory
                .path()
                .join(if existing { "existing" } else { "missing" });
            if existing {
                std::fs::write(&path, b"keep").unwrap();
            }
            let path_text = path.to_str().unwrap();
            let error = unstable
                .write_bytes_to_file(path_text, || {
                    Err::<Vec<u8>, _>(UnstableFsError::operation("bad payload".into()))
                })
                .unwrap_err();
            assert_eq!(error.to_string(), "bad payload");
            let mut results = FsOperationResults::default();
            assert_eq!(
                unstable
                    .write_bytes_to_file_new(&mut results, path_text, || Err("bad payload".into())),
                -1
            );
            assert_eq!(results.error_message(), "bad payload");
            if existing {
                assert_eq!(std::fs::read(&path).unwrap(), b"keep");
            } else {
                assert!(!path.exists());
            }
        }
    }

    #[test]
    fn unstable_io_errors_keep_legacy_text_and_status_getter_contract() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("missing");
        let path_text = path.to_str().unwrap();
        let host = host_fs(Policy::allow_all(), Arc::new(Env::ambient()));
        let unstable = UnstableFs(&host);
        assert_eq!(
            unstable
                .read_file_to_string(path_text)
                .unwrap_err()
                .to_string(),
            format!("Failed to read file: {path_text}")
        );
        let mut results = FsOperationResults {
            file_content: b"previous result".to_vec(),
            ..Default::default()
        };
        let os_error = std::fs::read(&path).unwrap_err();
        assert_eq!(unstable.read_file_to_bytes_new(&mut results, path_text), -1);
        assert_eq!(
            results.error_message(),
            format!("Failed to read file {path_text}: {os_error}")
        );
        assert_eq!(results.file_content(), b"previous result");
    }
}
