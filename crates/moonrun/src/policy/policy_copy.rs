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

//! An owned, process-boundary representation of the policy for one Run.
//!
//! Callers depend on the published policy and its lifetime, not on how it is
//! stored. The current implementation uses a temporary JSON file; a future
//! transport can replace that storage without changing policy serialization.

use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;

use super::config::{EnvConfig, PolicyConfig};

#[derive(Clone, Debug)]
pub(super) struct PolicyCopy {
    inner: Arc<PolicyCopyFile>,
}

#[derive(Debug)]
struct PolicyCopyFile {
    path: PathBuf,
    _file: tempfile::NamedTempFile,
}

impl PolicyCopy {
    pub(super) fn publish(config: &PolicyConfig) -> anyhow::Result<Self> {
        let mut inherited = config.clone();
        // Env owns the Run's current values. They cross the process boundary
        // through the normal process environment rather than the policy file.
        inherited.env = Some(EnvConfig {
            from_host: vec!["*".to_owned()],
            ..Default::default()
        });

        let mut file = tempfile::Builder::new()
            .prefix("moonrun-policy-")
            .suffix(".json")
            .tempfile()
            .context("failed to create temporary policy copy")?;
        serde_json::to_writer_pretty(file.as_file_mut(), &inherited)
            .context("failed to serialize inherited policy")?;
        file.as_file_mut()
            .flush()
            .context("failed to write temporary policy copy")?;

        let path = std::fs::canonicalize(file.path())
            .context("failed to resolve temporary policy copy")?;
        let wasi_preopen = std::fs::canonicalize(
            std::env::current_dir().context("failed to resolve the current directory")?,
        )
        .context("failed to resolve the WASI preopen directory")?;
        // WASI preopens the Run's current directory independently of Moonrun
        // Policy, so the copy must not be placed anywhere below it.
        anyhow::ensure!(
            !path.starts_with(&wasi_preopen),
            "temporary policy copy would be reachable through the WASI preopen"
        );

        Ok(Self {
            inner: Arc::new(PolicyCopyFile { path, _file: file }),
        })
    }

    pub(super) fn path(&self) -> &Path {
        &self.inner.path
    }

    pub(super) fn token(&self) -> &OsStr {
        self.inner.path.as_os_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_lives_until_its_last_owner_is_dropped() {
        let copy = PolicyCopy::publish(&PolicyConfig::default()).unwrap();
        let path = copy.path().to_owned();
        let second_owner = copy.clone();

        drop(copy);
        assert!(path.exists());

        drop(second_owner);
        assert!(!path.exists());
    }
}
