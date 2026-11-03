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
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::{fs, io};

use crate::async_sys::fs::stub;
use crate::policy::{FsIntents, FsPolicy};
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

/// Engine-neutral filesystem operations that enforce the runtime policy
/// before accessing the operating system's filesystem.
pub(crate) struct HostFs {
    policy: Option<FsPolicy>,
    environment: Arc<Env>,
    working_directory: Arc<WorkingDirectory>,
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
        self.authorize_read(path.as_os_str())?;
        Ok(fs::read(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn read_text(&self, path: &Path) -> Result<String, FsError> {
        self.authorize_read(path.as_os_str())?;
        Ok(fs::read_to_string(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn write_file(&self, path: &Path, contents: &[u8]) -> Result<(), FsError> {
        self.authorize_write(path.as_os_str())?;
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
        self.authorize_write(path.as_os_str())?;
        fs::write(self.resolve_path(path.as_os_str()), contents()?)
            .map_err(|error| FsError::Io(error).into())
    }

    pub(crate) fn create_directory_all(&self, path: &Path) -> Result<(), FsError> {
        self.authorize_write(path.as_os_str())?;
        Ok(fs::create_dir_all(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn remove_directory_all(&self, path: &Path) -> Result<(), FsError> {
        self.authorize_remove(path.as_os_str())?;
        Ok(fs::remove_dir_all(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn current_directory(&self) -> Result<PathBuf, FsError> {
        self.authorize_read(OsStr::new("."))?;
        Ok(self.working_directory.current_dir()?)
    }

    pub(crate) fn open_file(
        &self,
        path: &Path,
        options: FileOpenOptions,
    ) -> Result<fs::File, FsError> {
        self.authorize_path(
            path.as_os_str(),
            FsIntents::new(
                options.read,
                options.write
                    || options.append
                    || options.create
                    || options.create_new
                    || options.truncate,
            ),
        )?;
        Ok(fs::OpenOptions::new()
            .read(options.read)
            .write(options.write)
            .append(options.append)
            .create(options.create)
            .create_new(options.create_new)
            .truncate(options.truncate)
            .open(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn metadata(&self, path: &Path) -> Result<fs::Metadata, FsError> {
        self.authorize_read(path.as_os_str())?;
        Ok(fs::metadata(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn read_link(&self, path: &Path) -> Result<PathBuf, FsError> {
        self.authorize_entry_metadata(path.as_os_str())?;
        Ok(fs::read_link(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn read_directory(&self, path: &Path) -> Result<fs::ReadDir, FsError> {
        self.authorize_read(path.as_os_str())?;
        Ok(fs::read_dir(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn create_directory(&self, path: &Path) -> Result<(), FsError> {
        self.authorize_write_entry(path.as_os_str())?;
        Ok(fs::create_dir(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn rename(&self, source: &Path, target: &Path) -> Result<(), FsError> {
        self.authorize_rename(source.as_os_str(), target.as_os_str())?;
        Ok(super::os::rename(
            Path::new(&self.resolve_path(source.as_os_str())),
            Path::new(&self.resolve_path(target.as_os_str())),
            super::os::RenameOptions {
                replace: true,
                cross_device: super::os::CrossDevice::Reject,
            },
        )?)
    }

    pub(crate) fn remove_empty_directory(&self, path: &Path) -> Result<(), FsError> {
        self.authorize_remove(path.as_os_str())?;
        Ok(fs::remove_dir(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn unlink_file(&self, path: &Path) -> Result<(), FsError> {
        self.authorize_remove(path.as_os_str())?;
        Ok(fs::remove_file(self.resolve_path(path.as_os_str()))?)
    }
    pub(crate) fn authorize_open(
        &self,
        path: &OsStr,
        access: i32,
        create_mode: i32,
        append: bool,
    ) -> Result<(), FsError> {
        self.authorize_path(path, FsIntents::for_open(access, create_mode, append))
    }

    pub(super) fn authorize_metadata(&self, path: &OsStr) -> Result<(), FsError> {
        self.authorize_read(path)
    }

    pub(super) fn authorize_metadata_at(
        &self,
        base: Option<&Path>,
        path: &OsStr,
    ) -> Result<(), FsError> {
        let Some(policy) = &self.policy else {
            return Ok(());
        };
        let target = authorization_target_at(base, path);
        let resolved = resolve_from_directory(base, Path::new(path))
            .and_then(|path| canonicalize_existing_prefix(&path))
            .ok();
        policy
            .authorize(resolved.as_deref(), FsIntents::read(), &target)
            .map_err(|_| FsError::Denied)
    }

    pub(crate) fn authorize_resource_metadata(&self, path: Option<&Path>) -> Result<(), FsError> {
        let Some(policy) = &self.policy else {
            return Ok(());
        };
        match path {
            Some(path) => {
                let target = quote_os_str(path.as_os_str());
                let resolved = canonicalize_existing_prefix(path).ok();
                policy
                    .authorize(resolved.as_deref(), FsIntents::read(), &target)
                    .map_err(|_| FsError::Denied)
            }
            None => policy
                .authorize(None, FsIntents::read(), "\"<untracked resource>\"")
                .map_err(|_| FsError::Denied),
        }
    }

    pub(crate) fn authorize_read(&self, path: &OsStr) -> Result<(), FsError> {
        self.authorize_path(path, FsIntents::read())
    }

    pub(crate) fn authorize_write(&self, path: &OsStr) -> Result<(), FsError> {
        self.authorize_path(path, FsIntents::write())
    }

    pub(super) fn authorize_entry_metadata_at(
        &self,
        base: Option<&Path>,
        path: &OsStr,
    ) -> Result<(), FsError> {
        let Some(policy) = &self.policy else {
            return Ok(());
        };
        let target = authorization_target_at(base, path);
        let resolved = resolve_from_directory(base, Path::new(path))
            .and_then(|path| canonicalize_entry_path(&path))
            .ok();
        policy
            .authorize(resolved.as_deref(), FsIntents::read(), &target)
            .map_err(|_| FsError::Denied)
    }

    pub(super) fn authorize_entry_metadata(&self, path: &OsStr) -> Result<(), FsError> {
        self.authorize_entry(path, FsIntents::read())
    }

    pub(super) fn authorize_access(&self, path: &OsStr, access: i32) -> Result<(), FsError> {
        self.authorize_path(path, FsIntents::for_access_check(access))
    }

    pub(super) fn authorize_remove(&self, path: &OsStr) -> Result<(), FsError> {
        self.authorize_entry(path, FsIntents::write())
    }

    pub(super) fn authorize_rename(
        &self,
        old_path: &OsStr,
        new_path: &OsStr,
    ) -> Result<(), FsError> {
        self.authorize_entry(old_path, FsIntents::write())?;
        self.authorize_entry(new_path, FsIntents::write())
    }

    pub(crate) fn authorize_resource_lock(
        &self,
        path: Option<&Path>,
        exclusive: bool,
    ) -> Result<(), FsError> {
        if !exclusive {
            return Ok(());
        }
        let Some(policy) = &self.policy else {
            return Ok(());
        };
        match path {
            Some(path) => {
                let target = quote_os_str(path.as_os_str());
                let resolved = canonicalize_existing_prefix(path).ok();
                policy
                    .authorize(resolved.as_deref(), FsIntents::write(), &target)
                    .map_err(|_| FsError::Denied)
            }
            None => policy
                .authorize(None, FsIntents::write(), "\"<untracked resource>\"")
                .map_err(|_| FsError::Denied),
        }
    }

    pub(super) fn authorize_write_entry(&self, path: &OsStr) -> Result<(), FsError> {
        self.authorize_entry(path, FsIntents::write())
    }

    fn authorize_path(&self, path: &OsStr, intents: FsIntents) -> Result<(), FsError> {
        let Some(policy) = &self.policy else {
            return Ok(());
        };
        let target = quote_os_str(path);
        let resolved = self
            .working_directory
            .resolve_absolute(Path::new(path))
            .ok()
            .and_then(|path| canonicalize_existing_prefix(&path).ok());
        policy
            .authorize(resolved.as_deref(), intents, &target)
            .map_err(|_| FsError::Denied)
    }

    fn authorize_entry(&self, path: &OsStr, intents: FsIntents) -> Result<(), FsError> {
        let Some(policy) = &self.policy else {
            return Ok(());
        };
        let target = quote_os_str(path);
        let resolved = self
            .working_directory
            .resolve_absolute(Path::new(path))
            .ok()
            .and_then(|path| canonicalize_entry_path(&path).ok());
        policy
            .authorize(resolved.as_deref(), intents, &target)
            .map_err(|_| FsError::Denied)
    }
}

fn resolve_from_directory(base: Option<&Path>, path: &Path) -> Result<PathBuf, FsError> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        base.map(|base| base.join(path)).ok_or(FsError::Denied)
    }
}

fn canonicalize_existing_prefix(path: &Path) -> Result<PathBuf, FsError> {
    for ancestor in path.ancestors() {
        if let Ok(prefix) = std::fs::canonicalize(ancestor) {
            let suffix = path.strip_prefix(ancestor).map_err(|_| FsError::Denied)?;
            return Ok(normalize_path(prefix.join(suffix)));
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

fn canonicalize_entry_path(path: &Path) -> Result<PathBuf, FsError> {
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

fn quote_os_str(value: &OsStr) -> String {
    format!("{:?}", value.to_string_lossy())
}

fn authorization_target_at(base: Option<&Path>, path: &OsStr) -> String {
    base.map_or_else(
        || quote_os_str(OsStr::new("<untracked resource>")),
        |_| quote_os_str(path),
    )
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
    fn relative_at_authorization_uses_the_open_directory_path() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::write(allowed.join("input.txt"), "input").unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nread = [\"allowed\"]\n").unwrap();
        let (policy, _) = policy::load_file(&policy_file).unwrap();
        let host = host_fs(Some(policy), Arc::new(Env::ambient()));

        host.authorize_metadata_at(Some(&allowed), OsStr::new("input.txt"))
            .unwrap();
        assert!(matches!(
            host.authorize_metadata_at(None, OsStr::new("input.txt")),
            Err(FsError::Denied)
        ));
    }

    #[test]
    fn relative_at_authorization_preserves_untracked_diagnostic_target() {
        assert_eq!(
            authorization_target_at(None, OsStr::new("input.txt")),
            "\"<untracked resource>\""
        );
        assert_eq!(
            authorization_target_at(Some(Path::new("base")), OsStr::new("input.txt")),
            "\"input.txt\""
        );
    }

    #[cfg(unix)]
    #[test]
    fn resource_authorization_rechecks_saved_path() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&denied).unwrap();
        let resource_path = allowed.join("resource.txt");
        let denied_file = denied.join("target.txt");
        std::fs::write(&resource_path, "allowed").unwrap();
        std::fs::write(&denied_file, "denied").unwrap();
        let saved_path = std::fs::canonicalize(&resource_path).unwrap();
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

        let metadata = host.authorize_resource_metadata(Some(&saved_path));
        let lock = host.authorize_resource_lock(Some(&saved_path), true);
        assert!(matches!(metadata, Err(FsError::Denied)));
        assert!(matches!(lock, Err(FsError::Denied)));
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
            host.authorize_remove(denied_link.as_os_str()),
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

        host.authorize_remove(allowed_link.as_os_str()).unwrap();
        assert!(matches!(
            host.authorize_write(allowed_link.as_os_str()),
            Err(FsError::Denied)
        ));
    }
}
