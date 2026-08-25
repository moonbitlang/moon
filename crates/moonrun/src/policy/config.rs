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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PolicyConfig {
    pub(super) fs: Option<FsConfig>,
    pub(super) net: Option<NetConfig>,
    pub(super) env: Option<EnvConfig>,
    pub(super) process: Option<ProcessConfig>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FsConfig {
    #[serde(default)]
    pub(super) read: Vec<PathBuf>,
    #[serde(default)]
    pub(super) write: Vec<PathBuf>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NetConfig {
    #[serde(default)]
    pub(super) dns: Vec<String>,
    #[serde(default)]
    pub(super) connect: Vec<String>,
    #[serde(default)]
    pub(super) bind: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EnvConfig {
    #[serde(default)]
    pub(super) from_host: Vec<String>,
    #[serde(default)]
    pub(super) required_from_host: Vec<String>,
    #[serde(default)]
    pub(super) set: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProcessConfig {
    #[serde(default)]
    pub(super) spawn: bool,
    #[serde(default)]
    pub(super) allow: Vec<ProcessRuleConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProcessRuleConfig {
    pub(super) program: String,
    #[serde(default)]
    pub(super) args_prefix: Vec<String>,
}

impl PolicyConfig {
    pub(super) fn from_file(path: &Path) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read policy {}", path.display()))?;
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
            || contents.trim_start().starts_with('{')
        {
            serde_json::from_str(&contents)
                .with_context(|| format!("failed to parse JSON policy {}", path.display()))
        } else {
            toml::from_str(&contents)
                .with_context(|| format!("failed to parse policy {}", path.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_env_set_values() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(
            &policy_file,
            r#"
[env.set]
APP_ENV = "test"
"#,
        )
        .unwrap();

        let config = PolicyConfig::from_file(&policy_file).unwrap();
        let env = config.env.unwrap();

        assert_eq!(env.set.get("APP_ENV").map(String::as_str), Some("test"));
    }

    #[test]
    fn parses_toml_without_json_extension() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_file = tmp.path().join("policy");
        std::fs::write(
            &policy_file,
            r#"
[process]
spawn = true
"#,
        )
        .unwrap();

        let config = PolicyConfig::from_file(&policy_file).unwrap();

        assert!(config.process.unwrap().spawn);
    }

    #[test]
    fn parses_json_without_json_extension() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_file = tmp.path().join("policy");
        std::fs::write(
            &policy_file,
            r#"
{
  "env": {
    "set": {
      "APP_ENV": "test"
    }
  },
  "process": {
    "spawn": true
  }
}"#,
        )
        .unwrap();

        let config = PolicyConfig::from_file(&policy_file).unwrap();

        assert_eq!(
            config.env.unwrap().set.get("APP_ENV").map(String::as_str),
            Some("test")
        );
        assert!(config.process.unwrap().spawn);
    }

    #[test]
    fn parses_process_prefix_rules() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(
            &policy_file,
            r#"
[[process.allow]]
program = "git"
args_prefix = ["status", "--short"]
"#,
        )
        .unwrap();

        let config = PolicyConfig::from_file(&policy_file).unwrap();
        let process = config.process.unwrap();

        assert!(!process.spawn);
        assert_eq!(process.allow.len(), 1);
        assert_eq!(process.allow[0].program, "git");
        assert_eq!(process.allow[0].args_prefix, ["status", "--short"]);
    }

    #[test]
    fn process_prefix_defaults_to_any_arguments_and_requires_an_array_when_present() {
        let missing =
            serde_json::from_str::<PolicyConfig>(r#"{"process":{"allow":[{"program":"git"}]}}"#)
                .unwrap();
        assert!(missing.process.unwrap().allow[0].args_prefix.is_empty());

        let empty = serde_json::from_str::<PolicyConfig>(
            r#"{"process":{"allow":[{"program":"git","args_prefix":[]}]}}"#,
        )
        .unwrap();
        assert!(empty.process.unwrap().allow[0].args_prefix.is_empty());

        let string = serde_json::from_str::<PolicyConfig>(
            r#"{"process":{"allow":[{"program":"git","args_prefix":""}]}}"#,
        )
        .err()
        .unwrap();
        assert!(string.to_string().contains("expected a sequence"));
    }

    #[test]
    fn rejects_unknown_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(
            &policy_file,
            r#"
[env]
unknown = ["APP_ENV"]
"#,
        )
        .unwrap();

        let error = match PolicyConfig::from_file(&policy_file) {
            Ok(_) => panic!("expected unknown policy field to fail"),
            Err(error) => error,
        };

        let message = error
            .chain()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(message.contains("unknown field"), "{message}");
    }
}
