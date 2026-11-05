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

//! Versioned transport of resolved policy rules between OS processes.
//!
//! This format is independent of user policy syntax. Decode validates semantic
//! values and recompiles glob matchers without filesystem observation or Env setup.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::io::{Seek, Write};
use std::net::IpAddr;

use super::{Policy, Rules, fs::FsRules, net::NetRules, process::ProcessRules};
use std::sync::Arc;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Transfer {
    version: u32,
    rule_groups: Vec<RuleGroup>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RuleGroup {
    fs: Filesystem,
    net: Network,
    process: Process,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Filesystem {
    pub read: Vec<FsRule>,
    pub write: Vec<FsRule>,
}

#[derive(Deserialize, Serialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(super) enum FsRule {
    Any,
    // Serde's OsString representation preserves Unix bytes / Windows units and
    // rejects the other platform's encoding. Paths need not be Unicode.
    Path(OsString),
    Glob { base: OsString, pattern: String },
    // Keep the effect separate from pattern syntax; older readers reject this
    // variant instead of silently dropping an inherited exclusion.
    ExcludeGlob { base: OsString, pattern: String },
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Network {
    pub dns: Vec<DnsPattern>,
    pub connect: Vec<SocketRule>,
    pub bind: Vec<SocketRule>,
}

#[derive(Deserialize, Serialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(super) enum DnsPattern {
    Any,
    Exact(String),
    Subdomain(String),
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SocketRule {
    pub host: SocketHost,
    pub port: SocketPort,
}

#[derive(Deserialize, Serialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(super) enum SocketHost {
    Any,
    Ip(IpAddr),
    Name(DnsPattern),
}

#[derive(Deserialize, Serialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(super) enum SocketPort {
    Any,
    Exact(u16),
}

#[derive(Deserialize, Serialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(super) enum Process {
    AllowAll,
    Scoped(Vec<Command>),
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Command {
    pub program: String,
    pub args_prefix: Vec<String>,
}

impl Policy {
    pub(crate) fn from_transfer(contents: &[u8]) -> anyhow::Result<Self> {
        let transfer: Transfer =
            serde_json::from_slice(contents).context("failed to decode inherited policy")?;
        anyhow::ensure!(transfer.version == 3, "unsupported policy transfer version");
        anyhow::ensure!(
            !transfer.rule_groups.is_empty(),
            "inherited policy must contain a rule group"
        );
        let rule_groups = transfer
            .rule_groups
            .into_iter()
            .map(|rules| {
                Ok(Rules {
                    fs: Arc::new(FsRules::from_transfer(rules.fs)?),
                    net: Arc::new(NetRules::from_transfer(rules.net)?),
                    process: Arc::new(ProcessRules::from_transfer(rules.process)?),
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Self(rule_groups.into()))
    }

    pub(crate) fn to_transfer(&self) -> std::io::Result<Vec<u8>> {
        let transfer = Transfer {
            version: 3,
            rule_groups: self
                .0
                .iter()
                .map(|rules| RuleGroup {
                    fs: rules.fs.to_transfer(),
                    net: rules.net.to_transfer(),
                    process: rules.process.to_transfer(),
                })
                .collect(),
        };
        Ok(serde_json::to_vec(&transfer)?)
    }

    pub(crate) fn open_transfer(
        &self,
    ) -> std::io::Result<moonutil::policy_transport::PolicyTransfer> {
        let mut file = tempfile::tempfile()?;
        file.write_all(&self.to_transfer()?)?;
        file.flush()?;
        file.rewind()?;
        moonutil::policy_transport::PolicyTransfer::from_file(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::config::PolicyConfig;

    fn compile(config: PolicyConfig) -> Policy {
        crate::policy::source::resolve(config, std::path::Path::new("."))
            .unwrap()
            .policy
    }

    #[test]
    fn user_documents_are_not_inherited_restrictions() {
        for document in [
            br#"{}"#.as_slice(),
            br#"{"env":{"set":{"INJECTED":"value"}}}"#,
            br#"{"process":{"spawn":true}}"#,
        ] {
            assert!(Policy::from_transfer(document).is_err());
        }
    }

    #[test]
    fn inheritance_transport_is_not_a_user_policy_document() {
        let rules = compile(PolicyConfig::default());
        let contents = rules.open_transfer().unwrap().read().unwrap();
        assert!(serde_json::from_slice::<PolicyConfig>(&contents).is_err());
        let value: serde_json::Value = serde_json::from_slice(&contents).unwrap();
        assert!(value.get("env").is_none());
    }

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
        let rules = compile(serde_json::from_slice(input).unwrap());
        let encoded = rules.open_transfer().unwrap().read().unwrap();
        let decoded = Policy::from_transfer(&encoded).unwrap();
        assert_eq!(decoded.open_transfer().unwrap().read().unwrap(), encoded);
        for rules in [rules, decoded] {
            rules
                .filesystem()
                .authorize(Some(std::path::Path::new("file")), true, false, "file")
                .unwrap();
            assert!(
                rules
                    .filesystem()
                    .authorize(Some(std::path::Path::new("file")), false, true, "file")
                    .is_err()
            );
            #[cfg(unix)]
            rules
                .process()
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

    #[cfg(windows)]
    #[test]
    fn transfer_preserves_windows_command_prefix_decisions() {
        let rules = compile(
            serde_json::from_str(
                r#"{"process":{"allow":[{"program":"tool","args_prefix":["a b"]}]}}"#,
            )
            .unwrap(),
        );
        let contents = rules.open_transfer().unwrap().read().unwrap();
        let decoded = Policy::from_transfer(&contents).unwrap();
        for rules in [rules, decoded] {
            rules
                .process()
                .allows_windows(r#"tool.exe "a b" extra"#.as_ref())
                .unwrap();
            assert!(
                rules
                    .process()
                    .allows_windows("tool.exe a b".as_ref())
                    .is_err()
            );
            assert!(
                rules
                    .process()
                    .allows_windows(r#"other.exe "a b""#.as_ref())
                    .is_err()
            );
        }
    }

    #[test]
    fn transferred_roots_must_be_resolved() {
        for root in ["relative", "../escape"] {
            let input = Transfer {
                version: 3,
                rule_groups: vec![RuleGroup {
                    fs: Filesystem {
                        read: vec![FsRule::Path(root.into())],
                        write: vec![],
                    },
                    net: Network {
                        dns: vec![],
                        connect: vec![],
                        bind: vec![],
                    },
                    process: Process::Scoped(vec![]),
                }],
            };
            assert!(Policy::from_transfer(&serde_json::to_vec(&input).unwrap()).is_err());
        }
    }

    #[test]
    fn transfer_rejects_unknown_versions_fields_and_invalid_rules() {
        let rules = compile(PolicyConfig::default());
        let contents = rules.open_transfer().unwrap().read().unwrap();
        let original: serde_json::Value = serde_json::from_slice(&contents).unwrap();
        for (pointer, value) in [
            ("/version", serde_json::json!(99)),
            (
                "/rule_groups/0/process",
                serde_json::json!({"kind":"scoped","value":[{"program":"","args_prefix":[]}]}),
            ),
            (
                "/rule_groups/0/net/dns",
                serde_json::json!([{"kind":"exact","value":"Example.COM."}]),
            ),
            (
                "/rule_groups/0/net/bind",
                serde_json::json!([{"host":{"kind":"name","value":{"kind":"exact","value":"example.com"}},"port":{"kind":"any"}}]),
            ),
        ] {
            let mut invalid = original.clone();
            *invalid.pointer_mut(pointer).unwrap() = value;
            assert!(
                Policy::from_transfer(&serde_json::to_vec(&invalid).unwrap()).is_err(),
                "{pointer}"
            );
        }
        for pointer in [
            "",
            "/rule_groups/0",
            "/rule_groups/0/fs",
            "/rule_groups/0/net",
            "/rule_groups/0/process",
        ] {
            let mut invalid = original.clone();
            invalid
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("env".into(), serde_json::json!({"from_host":["*"]}));
            assert!(
                Policy::from_transfer(&serde_json::to_vec(&invalid).unwrap()).is_err(),
                "{pointer}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn transfer_preserves_native_path_bytes_without_filesystem_observation() {
        use std::os::unix::ffi::OsStringExt;
        let root = std::path::PathBuf::from(OsString::from_vec(
            b"/not-required-to-exist/allowed-\xff".to_vec(),
        ));
        let rules = Policy::new(
            FsRules {
                read_rules: vec![super::super::fs::FsRule::Path(root.clone())],
                write_rules: vec![],
            },
            NetRules::from_config(Default::default()).unwrap(),
            ProcessRules::from_config(Default::default()).unwrap(),
        );
        let contents = rules.open_transfer().unwrap().read().unwrap();
        let decoded = Policy::from_transfer(&contents).unwrap();
        decoded
            .filesystem()
            .authorize(Some(&root), true, false, "allowed")
            .unwrap();
        let other = std::path::PathBuf::from("/not-required-to-exist/allowed-�");
        assert!(
            decoded
                .filesystem()
                .authorize(Some(&other), true, false, "other")
                .is_err()
        );
    }

    #[test]
    fn inherited_transfer_outlives_the_canonical_representation() {
        let inheritance = compile(PolicyConfig::default());
        let transfer = inheritance.open_transfer().unwrap();

        drop(inheritance);

        let contents = transfer.read().unwrap();
        let decoded = Policy::from_transfer(&contents).unwrap();
        assert!(
            decoded
                .filesystem()
                .authorize(Some(std::path::Path::new("file")), true, false, "file")
                .is_err()
        );
    }

    #[test]
    fn each_transfer_has_an_independent_file_position() {
        let inheritance = compile(PolicyConfig::default());
        let first = inheritance.open_transfer().unwrap();
        let second = inheritance.open_transfer().unwrap();
        let first_contents = first.read().unwrap();
        let second_contents = second.read().unwrap();

        assert_eq!(first_contents, second_contents);
        assert!(!first_contents.is_empty());
    }
}
