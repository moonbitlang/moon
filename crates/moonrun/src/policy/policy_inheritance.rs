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

//! The canonical policy representation published for descendant Runs.
//!
//! Policy construction keeps immutable serialized bytes. A spawn job worker
//! materializes them into an anonymous OS-backed transfer only after it
//! recognizes a direct moonx spawn, so ordinary Runs do not depend on temporary
//! storage or expose a filesystem pathname.

use std::io::{Seek, Write};
use std::sync::Arc;

use anyhow::Context;

use super::config::{EnvConfig, PolicyConfig};

#[derive(Clone, Debug)]
pub(crate) struct PolicyInheritance {
    contents: Arc<[u8]>,
}

impl PolicyInheritance {
    pub(super) fn from_config(config: &PolicyConfig) -> anyhow::Result<Self> {
        let mut inherited = config.clone();
        // Env owns the Run's current values. They cross the process boundary
        // through the normal process environment rather than this payload.
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

    pub(crate) fn open_transfer(
        &self,
    ) -> std::io::Result<moonutil::policy_transport::PolicyTransfer> {
        let mut file = tempfile::tempfile()?;
        file.write_all(&self.contents)?;
        file.flush()?;
        file.rewind()?;
        moonutil::policy_transport::PolicyTransfer::from_file(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherited_transfer_outlives_the_canonical_representation() {
        let inheritance = PolicyInheritance::from_config(&PolicyConfig::default()).unwrap();
        let transfer = inheritance.open_transfer().unwrap();

        drop(inheritance);

        let contents = transfer.read().unwrap();
        let config: PolicyConfig = serde_json::from_slice(&contents).unwrap();
        assert_eq!(config.env.unwrap().from_host, ["*"]);
    }

    #[test]
    fn each_transfer_has_an_independent_file_position() {
        let inheritance = PolicyInheritance::from_config(&PolicyConfig::default()).unwrap();
        let first = inheritance.open_transfer().unwrap();
        let second = inheritance.open_transfer().unwrap();
        let first_contents = first.read().unwrap();
        let second_contents = second.read().unwrap();

        assert_eq!(first_contents, second_contents);
        assert!(!first_contents.is_empty());
    }
}
