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

//! A process-boundary representation of one Run's canonical Moonrun Policy.
//!
//! Policy construction keeps immutable serialized bytes in memory. A direct
//! moonx spawn publishes one temporary pathname for that child. This keeps the
//! transport replaceable without making ordinary policy-bearing Runs depend on
//! temporary storage.

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;

use super::config::{EnvConfig, PolicyConfig};

const COPY_PREFIX: &str = "moonrun-policy-";
const COPY_SUFFIX: &str = ".json";

#[derive(Clone, Debug)]
pub(super) struct PolicyCopy {
    contents: Arc<[u8]>,
}

impl PolicyCopy {
    pub(super) fn publish(config: &PolicyConfig) -> anyhow::Result<Self> {
        let mut inherited = config.clone();
        // Env owns the Run's current values. They cross the process boundary
        // through the normal process environment rather than the policy copy.
        inherited.env = Some(EnvConfig {
            from_host: vec!["*".to_owned()],
            ..Default::default()
        });

        let contents = serde_json::to_vec_pretty(&inherited)
            .context("failed to serialize inherited Moonrun Policy")?;
        Ok(Self {
            contents: contents.into(),
        })
    }

    /// Publish a copy for one direct moonx spawn.
    ///
    /// The pathname intentionally survives the publishing Run so a detached
    /// moonx can open it. The receiving moonrun consumes it at process entry.
    pub(super) fn publish_transfer(&self) -> std::io::Result<PathBuf> {
        let mut copy = tempfile::Builder::new()
            .prefix(COPY_PREFIX)
            .suffix(COPY_SUFFIX)
            .tempfile()?;
        copy.write_all(&self.contents)?;
        copy.flush()?;
        let path = std::fs::canonicalize(copy.path())?;
        let (file, _) = copy.keep().map_err(|error| error.error)?;
        drop(file);
        Ok(path)
    }
}

/// Consume the private pathname transport before the Run environment exists.
pub(crate) fn consume_transfer(token: OsString) -> anyhow::Result<Vec<u8>> {
    let path = PathBuf::from(token);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("invalid inherited Moonrun Policy copy path")?;
    anyhow::ensure!(
        name.starts_with(COPY_PREFIX) && name.ends_with(COPY_SUFFIX),
        "invalid inherited Moonrun Policy copy path"
    );
    let contents = std::fs::read(&path)
        .with_context(|| format!("failed to read inherited policy copy {}", path.display()))?;
    // The bytes are already owned by this process, so removing the transport
    // name before parsing prevents it from reaching the guest filesystem.
    let _ = std::fs::remove_file(path);
    Ok(contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_outlives_the_publisher_and_is_consumed_once() {
        let copy = PolicyCopy::publish(&PolicyConfig::default()).unwrap();
        let path = copy.publish_transfer().unwrap();

        drop(copy);
        assert!(path.exists());

        let contents = consume_transfer(path.clone().into_os_string()).unwrap();
        let config: PolicyConfig = serde_json::from_slice(&contents).unwrap();
        assert_eq!(config.env.unwrap().from_host, ["*"]);
        assert!(!path.exists());
    }

    #[test]
    fn each_spawn_gets_an_independent_transfer() {
        let copy = PolicyCopy::publish(&PolicyConfig::default()).unwrap();
        let first = copy.publish_transfer().unwrap();
        let second = copy.publish_transfer().unwrap();

        assert_ne!(first, second);

        consume_transfer(first.into_os_string()).unwrap();
        consume_transfer(second.into_os_string()).unwrap();
    }
}
