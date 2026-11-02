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

//! Immutable resolved restrictions and their OS-process transport adapter.
//!
//! Virtual children share these rules directly. Serialization is confined to
//! external process transfer; the representation contains no environment values
//! or provisioning instructions.

use anyhow::Context;
use std::io::{Seek, Write};
use std::sync::Arc;

use super::{FsPolicy, NetPolicy, Policy, ProcessPolicy, config::PolicyConfig};
use crate::runtime::EnvConfig;

#[derive(Clone, Debug)]
pub(crate) struct PolicyInheritance(Arc<ResolvedRestrictions>);

/// Constructible only from validated rules with absolute filesystem roots.
/// It cannot contain source-relative paths or environment provisioning.
#[derive(Debug)]
struct ResolvedRestrictions {
    fs: FsPolicy,
    net: NetPolicy,
    process: ProcessPolicy,
}

/// Input from an external process has to be decoded at Runtime construction;
/// an in-process child already has resolved rules and never enters that path.
#[derive(Debug)]
pub(crate) enum InheritedPolicyInput {
    Resolved(PolicyInheritance),
    Transfer(Vec<u8>),
}

impl PolicyInheritance {
    pub(super) fn from_resolved_config(config: PolicyConfig) -> anyhow::Result<Self> {
        anyhow::ensure!(
            config.env.is_none(),
            "resolved restrictions cannot provision Env"
        );
        let fs = config.fs.unwrap_or_default();
        anyhow::ensure!(
            fs.read.iter().chain(&fs.write).all(|root| {
                root.as_os_str() == std::ffi::OsStr::new("*") || root.is_absolute()
            }),
            "inherited filesystem roots must be absolute"
        );
        Ok(Self(Arc::new(ResolvedRestrictions {
            fs: FsPolicy::from_canonical_config(fs),
            net: NetPolicy::from_config(config.net.unwrap_or_default())?,
            process: ProcessPolicy::from_config(config.process.unwrap_or_default())?,
        })))
    }

    pub(crate) fn policy(&self) -> Policy {
        Policy {
            fs: Some(self.0.fs.clone()),
            net: Some(self.0.net.clone()),
            process: Some(self.0.process.clone()),
            policy_inheritance: Some(self.clone()),
        }
    }

    pub(crate) fn from_transfer(contents: &[u8]) -> anyhow::Result<(Self, EnvConfig)> {
        let mut config: PolicyConfig = serde_json::from_slice(contents)
            .context("failed to parse inherited JSON Moonrun Policy")?;
        let environment = config.env.take().unwrap_or_default();
        // Transfer roots were resolved by the granting Runtime. Filesystem I/O
        // here would let a replaced symlink redirect the inherited grant.
        Ok((Self::from_resolved_config(config)?, environment))
    }

    pub(crate) fn open_transfer(
        &self,
    ) -> std::io::Result<moonutil::policy_transport::PolicyTransfer> {
        let config = PolicyConfig {
            fs: Some(self.0.fs.to_config()),
            net: Some(self.0.net.to_config()),
            process: Some(self.0.process.to_config()),
            // Env travels through the normal process environment. This legacy
            // wire field selects that source; it never replays parent settings.
            env: Some(EnvConfig {
                from_host: vec!["*".into()],
                ..Default::default()
            }),
        };
        let mut file = tempfile::tempfile()?;
        serde_json::to_writer(&mut file, &config)?;
        file.flush()?;
        file.rewind()?;
        moonutil::policy_transport::PolicyTransfer::from_file(file)
    }

    #[cfg(test)]
    fn from_config(config: &PolicyConfig) -> anyhow::Result<Self> {
        super::source::resolve(config.clone(), std::path::Path::new(".")).map(|(rules, _)| rules)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_round_trip_preserves_resolved_rule_shapes() {
        // Exercise every encoding shape, including normalized names, IPv6,
        // wildcard ports, and arguments requiring Windows quoting.
        let input = br#"{
            "fs": {"read":["*"], "write":[]},
            "net": {
                "dns":["Example.COM.", "*.EXAMPLE.org", "*"],
                "connect":["api.example.com:443", "*.example.org:*", "127.0.0.1:80", "[::1]:443", "*:*"],
                "bind":["[::]:*", "127.0.0.1:0"]
            },
            "process":{"allow":[{"program":"a program", "args_prefix":["", "a b", "quote\"", "slash\\"]}]}
        }"#;
        let (rules, _) = PolicyInheritance::from_transfer(input).unwrap();
        let encoded = rules.open_transfer().unwrap().read().unwrap();
        let (decoded, _) = PolicyInheritance::from_transfer(&encoded).unwrap();
        assert_eq!(decoded.open_transfer().unwrap().read().unwrap(), encoded);
        let config: PolicyConfig = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(
            config.net.unwrap().connect,
            [
                "api.example.com:443",
                "*.example.org:*",
                "127.0.0.1:80",
                "[::1]:443",
                "*:*"
            ]
        );
        for rules in [rules, decoded] {
            rules
                .0
                .fs
                .authorize(
                    Some(std::path::Path::new("file")),
                    super::super::FsIntents::read(),
                    "file",
                )
                .unwrap();
            assert!(
                rules
                    .0
                    .fs
                    .authorize(
                        Some(std::path::Path::new("file")),
                        super::super::FsIntents::write(),
                        "file"
                    )
                    .is_err()
            );
            #[cfg(unix)]
            rules
                .0
                .process
                .allows_unix(
                    "a program".as_ref(),
                    &[
                        "a program".into(),
                        "".into(),
                        "a b".into(),
                        "quote\"".into(),
                        "slash\\".into(),
                    ],
                )
                .unwrap();
        }
    }

    #[test]
    fn transferred_roots_must_be_resolved() {
        for root in ["relative", "../escape"] {
            let input = serde_json::json!({"fs":{"read":[root]}}).to_string();
            assert!(PolicyInheritance::from_transfer(input.as_bytes()).is_err());
        }
    }

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
