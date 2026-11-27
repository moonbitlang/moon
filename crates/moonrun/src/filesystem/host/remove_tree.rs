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

//! The legacy recursive removal import composes directory listing, unlink and rmdir.

use super::{FsError, HostFs};
use crate::filesystem::os::openat::{self, Access, CreateMode, OpenOptions, Sync, TypeConstraint};
use crate::filesystem::{FsOperation, FsPath, FsTarget, directory_entries, os};
use std::{borrow::Cow, fs::File, io, path::Path, rc::Rc};

enum Step {
    Unlink,
    VisitDirectory,
    RemoveEmptyDirectory,
}

impl HostFs {
    pub(crate) fn remove_directory_all(&self, path: &Path) -> Result<(), FsError> {
        // Unix remove_dir_all also accepts a symlink as its root and unlinks it.
        #[cfg(unix)]
        if std::fs::symlink_metadata(self.resolve_path(path.as_os_str()))
            .is_ok_and(|metadata| metadata.is_symlink())
        {
            return self.unlink_file(path);
        }

        // Keep live parents while visiting their children. The final rmdir
        // rechecks the entry after traversal, including any policy change of
        // location. Never descend through a substituted symlink or junction.
        let mut pending = vec![(
            None::<Rc<File>>,
            path.as_os_str().to_owned(),
            Step::VisitDirectory,
        )];
        while let Some((parent, name, step)) = pending.pop() {
            let handle = parent.as_deref().map(os::raw_file);
            let (target, native) = match handle {
                Some(directory) => (
                    FsPath::AtHandle {
                        directory,
                        path: &name,
                    },
                    Cow::Borrowed(name.as_os_str()),
                ),
                None => (FsPath::Cwd(&name), Cow::Owned(self.resolve_path(&name))),
            };
            if matches!(step, Step::Unlink) {
                self.authorize(FsOperation::UnlinkFile(target))?;
                if let Err(error) = os::unlinkat::unlink_file_at(handle, &native, &os::DirectIo)
                    && error.kind() != io::ErrorKind::NotFound
                {
                    return Err(error.into());
                }
                continue;
            }
            self.authorize(FsOperation::RemoveDirectory(target))?;
            match os::unlinkat::remove_directory_at(handle, &native, &os::DirectIo) {
                Ok(()) => continue,
                Err(error)
                    if error.kind() == io::ErrorKind::NotFound
                        && (parent.is_some() || matches!(step, Step::RemoveEmptyDirectory)) =>
                {
                    continue;
                }
                Err(error)
                    if matches!(step, Step::VisitDirectory)
                        && error.kind() == io::ErrorKind::DirectoryNotEmpty => {}
                Err(error) => return Err(error.into()),
            }

            self.authorize(FsOperation::ListDirectory(FsTarget::Path(target)))?;
            // On Unix a trailing slash would make openat traverse a final symlink.
            // Remove the directory suffix only for the no-follow directory open.
            #[cfg(unix)]
            let entry = crate::filesystem::entry_before_directory_suffix(Path::new(&*native));
            #[cfg(unix)]
            let open_name = entry.as_deref().map(Path::as_os_str).unwrap_or(&native);
            #[cfg(windows)]
            let open_name = &*native;
            let file = match openat::open(
                handle,
                open_name,
                &OpenOptions {
                    access: Access::ListDirectory,
                    type_constraint: TypeConstraint::Directory,
                    follow_symlink: false,
                    is_async: false,
                    create: CreateMode::Open,
                    append: false,
                    sync: Sync::None,
                    permission: 0,
                },
                &os::DirectIo,
            ) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
                if file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    return Err(io::Error::from(io::ErrorKind::NotADirectory).into());
                }
            }
            let children = directory_entries::children(&file)?;
            let directory = Rc::new(file);
            pending.push((parent, name, Step::RemoveEmptyDirectory));
            pending.extend(children.into_iter().map(|(name, is_directory)| {
                let step = if is_directory {
                    Step::VisitDirectory
                } else {
                    Step::Unlink
                };
                (Some(Rc::clone(&directory)), name, step)
            }));
        }
        Ok(())
    }
}
