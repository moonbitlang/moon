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

//! Configuration and lifecycle of Moon-owned global cache roots.
//!
//! Cache contents are intentionally opaque here. Source and artifact stores
//! may choose their own representations without changing the CLI contract.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

const OWNERSHIP_MARKER: &str = ".moon-cache";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheKind {
    DependencySources,
    BuildArtifacts,
}

impl CacheKind {
    pub const fn environment_variable(self) -> &'static str {
        match self {
            Self::DependencySources => "MOON_DEP_CACHE",
            Self::BuildArtifacts => "MOON_BUILD_CACHE",
        }
    }

    const fn default_directory(self) -> &'static str {
        match self {
            Self::DependencySources => "deps",
            Self::BuildArtifacts => "build",
        }
    }

    const fn ownership(self) -> &'static [u8] {
        match self {
            Self::DependencySources => b"dependency-sources\n",
            Self::BuildArtifacts => b"build-artifacts\n",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheRoot {
    Disabled,
    Path(PathBuf),
}

pub fn resolve_cache_root(kind: CacheKind) -> anyhow::Result<CacheRoot> {
    let environment = kind.environment_variable();
    match std::env::var_os(environment) {
        Some(value) if value == "off" => Ok(CacheRoot::Disabled),
        Some(value) => {
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                bail!("{environment} must be an absolute path or `off`");
            }
            Ok(CacheRoot::Path(path))
        }
        None => Ok(CacheRoot::Path(
            crate::moon_dir::home()
                .join("cache")
                .join(kind.default_directory()),
        )),
    }
}

pub fn initialize_cache_root(kind: CacheKind, root: &Path) -> anyhow::Result<()> {
    match std::fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            bail!(
                "refusing to use symlinked Moon cache root `{}`",
                root.display()
            );
        }
        Ok(metadata) if !metadata.is_dir() => {
            bail!("Moon cache root `{}` is not a directory", root.display());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(root).with_context(|| {
                format!("failed to create Moon cache root `{}`", root.display())
            })?;
        }
        Err(error) => return Err(error.into()),
    }

    let marker = root.join(OWNERSHIP_MARKER);
    match std::fs::read(&marker) {
        Ok(contents) if contents == kind.ownership() => Ok(()),
        Ok(_) => bail!(
            "Moon cache root `{}` has the wrong ownership",
            root.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if std::fs::read_dir(root)?.next().transpose()?.is_some() {
                // Another initializer may have published the marker after our
                // first read. Accept that completed initialization.
                return match std::fs::read(&marker) {
                    Ok(contents) if contents == kind.ownership() => Ok(()),
                    Ok(_) => bail!(
                        "Moon cache root `{}` has the wrong ownership",
                        root.display()
                    ),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        bail!(
                            "refusing to use unrecognized Moon cache root `{}`",
                            root.display()
                        )
                    }
                    Err(error) => Err(error.into()),
                };
            }
            let parent = root.parent().with_context(|| {
                format!(
                    "Moon cache root `{}` has no parent directory",
                    root.display()
                )
            })?;
            // Stage outside the root so another initializer never mistakes our
            // temporary marker for user-owned contents.
            let mut staged = tempfile::NamedTempFile::new_in(parent)?;
            {
                use std::io::Write;
                staged.write_all(kind.ownership())?;
                staged.as_file().sync_all()?;
            }
            match staged.persist_noclobber(&marker) {
                Ok(_) => Ok(()),
                Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if std::fs::read(marker)? == kind.ownership() {
                        Ok(())
                    } else {
                        bail!(
                            "Moon cache root `{}` has the wrong ownership",
                            root.display()
                        )
                    }
                }
                Err(error) => Err(error.error.into()),
            }
        }
        Err(error) => Err(error.into()),
    }
}

pub fn make_cache_tree_readonly(root: &Path) -> anyhow::Result<()> {
    set_cache_tree_readonly(root, true)
}

pub fn make_cache_tree_writable(root: &Path) -> anyhow::Result<()> {
    set_cache_tree_readonly(root, false)
}

fn set_cache_tree_readonly(root: &Path, readonly: bool) -> anyhow::Result<()> {
    let metadata = std::fs::symlink_metadata(root)?;
    if metadata.file_type().is_symlink() {
        if readonly {
            bail!(
                "refusing to change permissions through cache symlink `{}`",
                root.display()
            );
        }
        // A cache clean removes this directory entry without following it.
        return Ok(());
    }
    if metadata.is_dir() {
        for entry in std::fs::read_dir(root)? {
            set_cache_tree_readonly(&entry?.path(), readonly)?;
        }
    }

    let mut permissions = metadata.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = permissions.mode();
        let mode = if readonly {
            mode & !0o222
        } else if metadata.is_dir() {
            mode | 0o700
        } else {
            mode | 0o200
        };
        permissions.set_mode(mode);
    }
    #[cfg(not(unix))]
    permissions.set_readonly(readonly);
    std::fs::set_permissions(root, permissions)?;
    Ok(())
}

pub fn clean_cache(kind: CacheKind) -> anyhow::Result<()> {
    let CacheRoot::Path(root) = resolve_cache_root(kind)? else {
        return Ok(());
    };
    let metadata = match std::fs::symlink_metadata(&root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        bail!(
            "refusing to clean symlinked Moon cache root `{}`",
            root.display()
        );
    }
    if metadata.is_dir() && std::fs::read_dir(&root)?.next().transpose()?.is_none() {
        std::fs::remove_dir(root)?;
        return Ok(());
    }
    if metadata.is_dir()
        && matches!(
            std::fs::read(root.join(OWNERSHIP_MARKER)),
            Ok(contents) if contents == kind.ownership()
        )
    {
        set_cache_tree_readonly(&root, false)?;
        std::fs::remove_dir_all(root)?;
        return Ok(());
    }
    bail!(
        "refusing to clean unrecognized Moon cache root `{}`",
        root.display()
    )
}
