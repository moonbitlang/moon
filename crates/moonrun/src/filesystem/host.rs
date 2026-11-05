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

//! Runtime filesystem interpretation and authorization, shared across import families.
//! ABI adapters own descriptor rights, value conversion, and error presentation.
//! Standard-library operations and `os` provide the filesystem mechanisms.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, io};

#[cfg(unix)]
use super::path::entry_before_directory_suffix;
use super::{FsOperation, FsPath, FsTarget};
use crate::async_sys::fs::stub;
use crate::policy::FsPolicy;
use crate::runtime::{Env, WorkingDirectory};

/// Preserve policy denial separately from OS errors for each caller's ABI.
#[derive(Debug, thiserror::Error)]
pub(crate) enum FsError {
    #[error("filesystem access denied by policy")]
    Denied,
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// One source for both native open options and their authorization intent.
pub(crate) struct FileOpenOptions {
    pub(crate) read: bool,
    pub(crate) write: bool,
    pub(crate) append: bool,
    pub(crate) create: bool,
    pub(crate) create_new: bool,
    pub(crate) truncate: bool,
}

/// An acquired directory, independent of guest descriptor numbering and rights.
/// Its pathname is queried only for authorization and path-only operations.
pub(crate) struct Directory {
    file: fs::File,
}

impl Directory {
    /// Runtime configuration acquires a preopen without granting guest access.
    /// HostFs still authorizes each operation using its requested effects.
    pub(crate) fn preopen(path: &Path) -> io::Result<Self> {
        use super::os::{
            DirectIo,
            openat::{self, Access, CreateMode, OpenOptions, Sync, TypeConstraint},
        };
        Ok(Self {
            file: openat::open(
                None,
                path.as_os_str(),
                &OpenOptions {
                    access: Access::TraverseDirectory,
                    type_constraint: TypeConstraint::Directory,
                    follow_symlink: true,
                    is_async: false,
                    create: CreateMode::Open,
                    append: false,
                    sync: Sync::None,
                    permission: 0,
                },
                &DirectIo,
            )?,
        })
    }

    pub(crate) fn path(&self) -> Result<PathBuf, FsError> {
        super::path::directory_path(super::os::raw_file(&self.file))
    }
}

/// Keep Unix relative traversal intact. Absolute paths and Windows DOS paths
/// need conversion to a name relative to the acquired handle. Resolve only the
/// parent: following the final link here would change unlink and exclusive-open
/// semantics before their OS operation gets to interpret the entry.
fn relative_path(base: &Path, path: &Path) -> Result<PathBuf, FsError> {
    #[cfg(unix)]
    if !path.is_absolute() {
        return Ok(if path.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            path.to_owned()
        });
    }
    #[cfg(windows)]
    let host_path = std::path::absolute(dunce::simplified(base).join(path))?;
    #[cfg(unix)]
    let host_path = path.to_owned();
    let resolved = if cfg!(unix) && host_path.starts_with(base) {
        host_path
    } else if dunce::simplified(&host_path) == dunce::simplified(base) {
        base.to_owned()
    } else if let Some(name) = host_path.file_name() {
        let parent = host_path.parent().ok_or(FsError::Denied)?;
        fs::canonicalize(parent)?.join(name)
    } else {
        // A terminal `..` names a directory, not a removable entry.
        fs::canonicalize(&host_path)?
    };
    let relative = resolved
        .strip_prefix(base)
        .map_err(|_| FsError::Denied)?
        .to_path_buf();
    #[cfg(unix)]
    let relative = {
        let mut relative = relative;
        if relative.as_os_str().is_empty() {
            relative.push(".");
        } else if entry_before_directory_suffix(path).is_some() {
            // strip_prefix discards directory suffixes. Preserve traversal and
            // a final dot (even in /./), which rmdir must not turn into deletion.
            let trailing_dot = path
                .as_os_str()
                .as_encoded_bytes()
                .rsplit(|byte| *byte == b'/')
                .find(|component| !component.is_empty())
                .is_some_and(|component| component == b".");
            relative.push(if trailing_dot { "." } else { "" });
        }
        relative
    };
    Ok(relative)
}

/// Engine-neutral filesystem operations that enforce the runtime policy
/// before accessing the operating system's filesystem.
pub(crate) struct HostFs {
    pub(super) policy: Option<FsPolicy>,
    environment: Arc<Env>,
    pub(super) working_directory: Arc<WorkingDirectory>,
}

impl HostFs {
    pub(crate) fn resolve_path(&self, path: &OsStr) -> OsString {
        self.working_directory.resolve_path(path)
    }

    pub(crate) fn new(
        policy: Option<FsPolicy>,
        environment: Arc<Env>,
        working_directory: Arc<WorkingDirectory>,
    ) -> Self {
        Self {
            policy,
            environment,
            working_directory,
        }
    }

    pub(crate) fn temp_dir(&self) -> Result<OsString, FsError> {
        // Native Windows selection has fallbacks beyond TMP and TEMP. Preserve
        // those only for Ambient; an owned Env must never consult process state.
        if self.environment.is_ambient() {
            return stub::get_tmp_path().map_err(Into::into);
        }
        #[cfg(unix)]
        {
            stub::get_tmp_path_from_env(self.environment.get("TMPDIR".as_ref())).map_err(Into::into)
        }
        #[cfg(windows)]
        {
            stub::get_tmp_path_from_env(
                self.environment.get("TMP".as_ref()),
                self.environment.get("TEMP".as_ref()),
            )
            .map_err(Into::into)
        }
    }

    pub(crate) fn read_file(&self, path: &Path) -> Result<Vec<u8>, FsError> {
        self.authorize(FsOperation::Read(FsPath::Cwd(path.as_os_str())))?;
        Ok(fs::read(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn read_text(&self, path: &Path) -> Result<String, FsError> {
        self.authorize(FsOperation::Read(FsPath::Cwd(path.as_os_str())))?;
        Ok(fs::read_to_string(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn write_file(&self, path: &Path, contents: &[u8]) -> Result<(), FsError> {
        self.authorize(FsOperation::WriteFile(FsPath::Cwd(path.as_os_str())))?;
        Ok(fs::write(self.resolve_path(path.as_os_str()), contents)?)
    }

    pub(crate) fn write_file_with<E>(
        &self,
        path: &Path,
        contents: impl FnOnce() -> Result<Vec<u8>, E>,
    ) -> Result<(), E>
    where
        E: From<FsError>,
    {
        // Authorize before decoding an untrusted payload. Decode before opening
        // the target so a conversion failure cannot create or truncate a file.
        self.authorize(FsOperation::WriteFile(FsPath::Cwd(path.as_os_str())))?;
        fs::write(self.resolve_path(path.as_os_str()), contents()?)
            .map_err(|error| FsError::Io(error).into())
    }

    pub(crate) fn create_directory_all(&self, path: &Path) -> Result<(), FsError> {
        self.authorize(FsOperation::CreateDirectories(FsPath::Cwd(
            path.as_os_str(),
        )))?;
        Ok(fs::create_dir_all(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn remove_directory_all(&self, path: &Path) -> Result<(), FsError> {
        self.authorize(FsOperation::ChangeEntry(FsPath::Cwd(path.as_os_str())))?;
        Ok(fs::remove_dir_all(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn current_directory(&self) -> Result<PathBuf, FsError> {
        self.authorize(FsOperation::Read(FsPath::Cwd(OsStr::new("."))))?;
        Ok(self.working_directory.current_dir()?)
    }

    pub(crate) fn open_file_at(
        &self,
        directory: &Directory,
        path: &Path,
        options: FileOpenOptions,
    ) -> Result<fs::File, FsError> {
        use super::os::{
            DirectIo,
            openat::{self, Access, CreateMode, OpenOptions, Sync, TypeConstraint},
        };
        let base = directory.path()?;
        let path = relative_path(&base, path)?;
        self.authorize(FsOperation::OpenWithOptions {
            path: FsPath::At {
                base: &base,
                path: path.as_os_str(),
            },
            options: &options,
        })?;
        let write = options.write || options.append;
        // Preserve std::OpenOptions validation before reaching a destructive
        // native open. Native async jobs have their own scalar-option contract.
        if (!write && (!options.read || options.create || options.create_new || options.truncate))
            || (options.append && options.truncate && !options.create_new)
        {
            return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
        }
        Ok(openat::open(
            Some(super::os::raw_file(&directory.file)),
            path.as_os_str(),
            &OpenOptions {
                access: if options.read && write {
                    Access::ReadWrite
                } else if write {
                    Access::Write
                } else {
                    Access::Read
                },
                type_constraint: TypeConstraint::Any,
                follow_symlink: true,
                is_async: false,
                create: if options.create_new {
                    CreateMode::Exclusive
                } else {
                    match (options.create, options.truncate) {
                        (false, false) => CreateMode::Open,
                        (false, true) => CreateMode::Truncate,
                        (true, false) => CreateMode::Create,
                        (true, true) => CreateMode::CreateOrTruncate,
                    }
                },
                append: options.append,
                sync: Sync::None,
                permission: 0o666,
            },
            &DirectIo,
        )?)
    }

    pub(crate) fn open_directory_at(
        &self,
        directory: &Directory,
        path: &Path,
    ) -> Result<Directory, FsError> {
        use super::os::{
            DirectIo,
            openat::{self, Access, CreateMode, OpenOptions, Sync, TypeConstraint},
        };
        let base = directory.path()?;
        let path = relative_path(&base, path)?;
        self.authorize(FsOperation::Read(FsPath::At {
            base: &base,
            path: path.as_os_str(),
        }))?;
        // NtCreateFile does not perform DOS dot-component normalization. The
        // normalized path is empty when opening the acquired directory itself.
        let file = if path.as_os_str().is_empty() {
            directory.file.try_clone()?
        } else {
            openat::open(
                Some(super::os::raw_file(&directory.file)),
                path.as_os_str(),
                &OpenOptions {
                    access: Access::TraverseDirectory,
                    type_constraint: TypeConstraint::Directory,
                    follow_symlink: true,
                    is_async: false,
                    create: CreateMode::Open,
                    append: false,
                    sync: Sync::None,
                    permission: 0,
                },
                &DirectIo,
            )?
        };
        Ok(Directory { file })
    }

    pub(crate) fn metadata(&self, path: &Path) -> Result<fs::Metadata, FsError> {
        self.authorize(FsOperation::Metadata(FsTarget::Path(FsPath::Cwd(
            path.as_os_str(),
        ))))?;
        Ok(fs::metadata(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn read_link(&self, path: &Path) -> Result<PathBuf, FsError> {
        self.authorize(FsOperation::InspectEntry(FsPath::Cwd(path.as_os_str())))?;
        Ok(fs::read_link(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn read_directory(&self, path: &Path) -> Result<fs::ReadDir, FsError> {
        self.authorize(FsOperation::Read(FsPath::Cwd(path.as_os_str())))?;
        Ok(fs::read_dir(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn create_directory(&self, path: &Path) -> Result<(), FsError> {
        self.authorize(FsOperation::ChangeEntry(FsPath::Cwd(path.as_os_str())))?;
        Ok(fs::create_dir(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn rename(&self, source: &Path, target: &Path) -> Result<(), FsError> {
        self.authorize(FsOperation::Rename {
            source: FsPath::Cwd(source.as_os_str()),
            destination: FsPath::Cwd(target.as_os_str()),
        })?;
        Ok(super::os::rename(
            Path::new(&self.resolve_path(source.as_os_str())),
            Path::new(&self.resolve_path(target.as_os_str())),
            super::os::RenameOptions {
                replace: true,
                cross_device: super::os::CrossDevice::Reject,
            },
        )?)
    }

    pub(crate) fn remove_directory_at(
        &self,
        directory: &Directory,
        path: &Path,
    ) -> Result<(), FsError> {
        let base = directory.path()?;
        let path = relative_path(&base, path)?;
        self.authorize(FsOperation::ChangeEntry(FsPath::At {
            base: &base,
            path: path.as_os_str(),
        }))?;
        Ok(super::os::unlinkat::remove_directory_at(
            Some(super::os::raw_file(&directory.file)),
            path.as_os_str(),
            &super::os::DirectIo,
        )?)
    }

    pub(crate) fn unlink_file_at(&self, directory: &Directory, path: &Path) -> Result<(), FsError> {
        let base = directory.path()?;
        let path = relative_path(&base, path)?;
        self.authorize(FsOperation::ChangeEntry(FsPath::At {
            base: &base,
            path: path.as_os_str(),
        }))?;
        Ok(super::os::unlinkat::unlink_file_at(
            Some(super::os::raw_file(&directory.file)),
            path.as_os_str(),
            &super::os::DirectIo,
        )?)
    }

    pub(crate) fn unlink_file(&self, path: &Path) -> Result<(), FsError> {
        self.authorize(FsOperation::ChangeEntry(FsPath::Cwd(path.as_os_str())))?;
        Ok(fs::remove_file(self.resolve_path(path.as_os_str()))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{self, Policy};

    fn host_fs(policy: Option<Policy>, environment: Arc<Env>) -> HostFs {
        HostFs::new(
            policy.as_ref().map(Policy::filesystem),
            environment,
            Arc::new(WorkingDirectory::Ambient),
        )
    }

    #[test]
    fn ambient_temp_dir_uses_native_path() {
        let host = host_fs(None, Arc::new(Env::ambient()));

        assert_eq!(host.temp_dir().unwrap(), stub::get_tmp_path().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn owned_temp_dir_uses_runtime_environment() {
        use std::os::unix::ffi::OsStrExt;

        let environment = Arc::new(Env::owned([("TMPDIR".into(), "/runtime/tmp".into())]).unwrap());
        let host = host_fs(None, environment);

        assert_eq!(host.temp_dir().unwrap().as_bytes(), b"/runtime/tmp/");
    }

    #[cfg(unix)]
    #[test]
    fn temp_dir_reflects_runtime_environment_changes() {
        use std::os::unix::ffi::OsStrExt;

        let environment = Arc::new(Env::owned([("TMPDIR".into(), "/first".into())]).unwrap());
        let host = host_fs(None, Arc::clone(&environment));

        assert_eq!(host.temp_dir().unwrap().as_bytes(), b"/first/");
        environment.set("TMPDIR".into(), "/second".into()).unwrap();
        assert_eq!(host.temp_dir().unwrap().as_bytes(), b"/second/");
        environment.unset("TMPDIR".as_ref()).unwrap();
        assert_eq!(
            host.temp_dir().unwrap(),
            stub::get_tmp_path_from_env(None).unwrap()
        );
    }

    #[cfg(all(unix, not(target_os = "android")))]
    #[test]
    fn empty_owned_environment_uses_default_temp_dir() {
        use std::os::unix::ffi::OsStrExt;

        let environment = Arc::new(Env::owned(Vec::<(OsString, OsString)>::new()).unwrap());
        let host = host_fs(None, environment);

        assert_eq!(host.temp_dir().unwrap().as_bytes(), b"/tmp/");
    }

    #[cfg(windows)]
    #[test]
    fn owned_temp_dir_requires_configured_windows_environment() {
        let environment = Arc::new(Env::owned(Vec::<(OsString, OsString)>::new()).unwrap());
        let host = host_fs(None, Arc::clone(&environment));

        assert!(matches!(host.temp_dir(), Err(FsError::Denied)));
        environment.set("TEMP".into(), "C:/Temp".into()).unwrap();
        assert_eq!(host.temp_dir().unwrap().to_string_lossy(), "C:/Temp\\");
        environment.unset("TEMP".as_ref()).unwrap();
        assert!(matches!(host.temp_dir(), Err(FsError::Denied)));
    }

    #[cfg(windows)]
    #[test]
    fn empty_tmp_falls_back_to_temp() {
        let environment = Arc::new(
            Env::owned([
                ("TMP".into(), "".into()),
                ("TEMP".into(), "C:/Fallback".into()),
            ])
            .unwrap(),
        );
        let host = host_fs(None, environment);

        assert_eq!(host.temp_dir().unwrap().to_string_lossy(), "C:/Fallback\\");
    }

    #[test]
    fn create_mode_requires_write_even_for_read_access() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nread = [\"allowed\"]\n").unwrap();
        let (policy, _) = policy::load_file(&policy_file).unwrap();
        let host = host_fs(Some(policy), Arc::new(Env::ambient()));

        assert!(matches!(
            host.authorize(FsOperation::OpenAsync {
                path: FsPath::Cwd(allowed.join("new.txt").as_os_str()),
                access: 0,
                create_mode: 1,
                read_metadata: false,
            }),
            Err(FsError::Denied)
        ));
    }

    #[test]
    fn directory_write_grant_allows_removing_its_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nwrite = [\"root\"]\n").unwrap();
        let (policy, _) = policy::load_file(&policy_file).unwrap();
        let host = host_fs(Some(policy), Arc::new(Env::ambient()));

        host.remove_directory_all(&root).unwrap();

        assert!(!root.exists());
    }

    #[test]
    fn directory_write_grant_allows_moving_its_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let destination = tmp.path().join("moved");
        std::fs::create_dir(&root).unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(
            &policy_file,
            "[fs]\nwrite = [\"root\", { glob = \"moved\" }]\n",
        )
        .unwrap();
        let (policy, _) = policy::load_file(&policy_file).unwrap();
        let host = host_fs(Some(policy), Arc::new(Env::ambient()));

        host.rename(&root, &destination).unwrap();

        assert!(!root.exists());
        assert!(destination.is_dir());
    }

    #[test]
    fn wildcard_write_grant_allows_creation_outside_policy_directory() {
        let config = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let policy_file = config.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nwrite = [\"*\"]\n").unwrap();
        let (policy, _) = policy::load_file(&policy_file).unwrap();
        let host = host_fs(Some(policy), Arc::new(Env::ambient()));
        let target = outside.path().join("file");

        host.write_file(&target, b"allowed").unwrap();

        assert_eq!(std::fs::read(target).unwrap(), b"allowed");
    }

    #[test]
    fn relative_at_authorization_uses_the_open_directory_path() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::write(allowed.join("input.txt"), "input").unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nread = [\"allowed\"]\n").unwrap();
        let (policy, _) = policy::load_file(&policy_file).unwrap();
        let host = host_fs(Some(policy), Arc::new(Env::ambient()));

        host.authorize(FsOperation::Read(FsPath::At {
            base: &allowed,
            path: OsStr::new("input.txt"),
        }))
        .unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn unrestricted_relative_lookup_needs_no_directory_path() {
        let directory = tempfile::tempdir().unwrap();
        let file = fs::File::open(directory.path()).unwrap();
        fs::remove_dir(directory.path()).unwrap();
        let host = host_fs(None, Arc::new(Env::ambient()));

        host.authorize(FsOperation::Metadata(FsTarget::Path(FsPath::AtHandle {
            directory: super::super::os::raw_file(&file),
            path: OsStr::new("."),
        })))
        .unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn restricted_absolute_lookup_still_checks_its_directory_handle() {
        let directory = tempfile::tempdir().unwrap();
        let file = fs::File::open(directory.path()).unwrap();
        fs::remove_dir(directory.path()).unwrap();
        let environment = Arc::new(Env::ambient());
        let (policy, _) = policy::load_json(br#"{"fs":{"read":["*"]}}"#, &environment).unwrap();
        let host = host_fs(Some(policy), environment);

        let result = host.authorize(FsOperation::Metadata(FsTarget::Path(FsPath::AtHandle {
            directory: super::super::os::raw_file(&file),
            path: OsStr::new("/"),
        })));
        assert!(result.is_err(), "{result:?}");
    }

    #[cfg(unix)]
    #[test]
    fn resource_authorization_rechecks_saved_path() {
        use crate::filesystem::FileGrant;

        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&denied).unwrap();
        let resource_path = allowed.join("resource.txt");
        let denied_file = denied.join("target.txt");
        std::fs::write(&resource_path, "allowed").unwrap();
        std::fs::write(&denied_file, "denied").unwrap();
        let grant = FileGrant::from_opened_path(Some(resource_path.clone()));
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(
            &policy_file,
            "[fs]\nread = [\"allowed\"]\nwrite = [\"allowed\"]\n",
        )
        .unwrap();
        let (policy, _) = policy::load_file(&policy_file).unwrap();
        let host = host_fs(Some(policy), Arc::new(Env::ambient()));

        std::fs::remove_file(&resource_path).unwrap();
        std::os::unix::fs::symlink(&denied_file, &resource_path).unwrap();

        let metadata = host.authorize(FsOperation::Metadata(FsTarget::File(&grant)));
        let lock = host.authorize(FsOperation::Lock {
            file: &grant,
            exclusive: true,
        });
        assert!(matches!(metadata, Err(FsError::Denied)));
        assert!(matches!(lock, Err(FsError::Denied)));
    }

    #[cfg(unix)]
    #[test]
    fn creation_policy_does_not_walk_past_unresolved_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&denied).unwrap();
        let link = allowed.join("link");
        std::os::unix::fs::symlink("../denied/missing", &link).unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nwrite = [\"allowed\"]\n").unwrap();
        let (policy, _) = policy::load_file(&policy_file).unwrap();
        let host = host_fs(Some(policy), Arc::new(Env::ambient()));

        assert!(matches!(
            host.write_file(&link, b"escape"),
            Err(FsError::Denied)
        ));
        let directory = Directory::preopen(&allowed).unwrap();
        assert!(matches!(
            host.open_file_at(
                &directory,
                Path::new("link"),
                FileOpenOptions {
                    read: false,
                    write: true,
                    append: false,
                    create: true,
                    create_new: false,
                    truncate: false,
                }
            ),
            Err(FsError::Denied)
        ));
        // Walking past a missing descendant must still inspect its link ancestor.
        assert!(matches!(
            host.create_directory_all(&link.join("child")),
            Err(FsError::Denied)
        ));
        for path in ["link/", "link/.", "new/../link/child"] {
            assert!(matches!(
                host.create_directory_all(&allowed.join(path)),
                Err(FsError::Denied)
            ));
        }
        assert!(!denied.join("missing").exists());
        assert!(!allowed.join("new").exists());

        std::os::unix::fs::symlink("loop", allowed.join("loop")).unwrap();
        assert!(matches!(
            host.authorize(FsOperation::Write(FsPath::Cwd(
                allowed.join("loop").as_os_str()
            ))),
            Err(FsError::Denied)
        ));
        host.unlink_file(&allowed.join("loop")).unwrap();

        host.create_directory_all(&allowed.join("new/child"))
            .unwrap();
        host.write_file(&allowed.join("new/file"), b"allowed")
            .unwrap();
        let renamed = allowed.join("renamed");
        host.rename(&link, &renamed).unwrap();
        assert_eq!(
            std::fs::read_link(&renamed).unwrap(),
            Path::new("../denied/missing")
        );
        host.unlink_file(&renamed).unwrap();
        assert!(std::fs::symlink_metadata(renamed).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn unrestricted_creation_keeps_native_dangling_link_behavior() {
        let tmp = tempfile::tempdir().unwrap();
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink("missing", &link).unwrap();
        let host = host_fs(None, Arc::new(Env::ambient()));

        host.write_file(&link, b"native").unwrap();
        assert_eq!(
            std::fs::read(tmp.path().join("missing")).unwrap(),
            b"native"
        );
        assert!(std::fs::symlink_metadata(link).unwrap().is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn directory_suffix_policy_rejects_dangling_links_but_allows_new_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&denied).unwrap();
        std::os::unix::fs::symlink("../denied/new", allowed.join("link")).unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nwrite = [\"allowed\"]\n").unwrap();
        let (policy, _) = policy::load_file(&policy_file).unwrap();
        let host = host_fs(Some(policy), Arc::new(Env::ambient()));
        let link = allowed.join("link/");
        assert!(matches!(host.create_directory(&link), Err(FsError::Denied)));
        let source = allowed.join("source");
        std::fs::create_dir(&source).unwrap();
        assert!(matches!(host.rename(&source, &link), Err(FsError::Denied)));
        assert!(source.is_dir());
        assert!(!denied.join("new").exists());

        host.create_directory(&allowed.join("new/")).unwrap();
        host.rename(&source, &allowed.join("new/")).unwrap();
        assert!(!source.exists());
        assert!(allowed.join("new").is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn remove_policy_checks_link_path_not_target() {
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
        let (policy, _) = policy::load_file(&policy_file).unwrap();
        let host = host_fs(Some(policy), Arc::new(Env::ambient()));

        assert!(matches!(
            host.authorize(FsOperation::ChangeEntry(FsPath::Cwd(
                denied_link.as_os_str()
            ))),
            Err(FsError::Denied)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn remove_policy_checks_link_entry_not_target() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&denied).unwrap();
        let denied_file = denied.join("target.txt");
        let allowed_link = allowed.join("link.txt");
        std::fs::write(&denied_file, "target").unwrap();
        std::os::unix::fs::symlink(&denied_file, &allowed_link).unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nwrite = [\"allowed\"]\n").unwrap();
        let (policy, _) = policy::load_file(&policy_file).unwrap();
        let host = host_fs(Some(policy), Arc::new(Env::ambient()));

        host.authorize(FsOperation::ChangeEntry(FsPath::Cwd(
            allowed_link.as_os_str(),
        )))
        .unwrap();
        assert!(matches!(
            host.authorize(FsOperation::Write(FsPath::Cwd(allowed_link.as_os_str()))),
            Err(FsError::Denied)
        ));
    }
}
