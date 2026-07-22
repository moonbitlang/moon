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

use std::path::PathBuf;

use anyhow::bail;

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
        std::fs::remove_dir_all(root)?;
        return Ok(());
    }
    bail!(
        "refusing to clean unrecognized Moon cache root `{}`",
        root.display()
    )
}
