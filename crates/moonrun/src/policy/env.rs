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

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use super::config::EnvConfig;

#[derive(Clone, Debug)]
pub(super) struct EnvPolicy {
    vars: Arc<Mutex<BTreeMap<String, String>>>,
}

impl EnvPolicy {
    pub(super) fn from_config(config: EnvConfig) -> anyhow::Result<Self> {
        let mut vars = BTreeMap::new();

        let copy_all = config.from_host.iter().any(|name| name == "*");
        if copy_all {
            vars.extend(std::env::vars());
        }

        copy_host_names(&mut vars, &config.from_host, false)?;
        copy_host_names(&mut vars, &config.required_from_host, true)?;

        for (name, value) in config.set {
            vars.insert(name, value);
        }

        Ok(Self {
            vars: Arc::new(Mutex::new(vars)),
        })
    }

    pub(super) fn vars(&self) -> Vec<(String, String)> {
        self.vars
            .lock()
            .unwrap()
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    }

    pub(super) fn get(&self, name: &str) -> Option<String> {
        self.vars.lock().unwrap().get(name).cloned()
    }

    pub(super) fn contains(&self, name: &str) -> bool {
        self.vars.lock().unwrap().contains_key(name)
    }

    pub(super) fn set(&self, name: String, value: String) {
        self.vars.lock().unwrap().insert(name, value);
    }

    pub(super) fn unset(&self, name: &str) {
        self.vars.lock().unwrap().remove(name);
    }
}

fn copy_host_names(
    vars: &mut BTreeMap<String, String>,
    names: &[String],
    required: bool,
) -> anyhow::Result<()> {
    let mut seen = BTreeSet::new();
    for name in names {
        if name == "*" {
            continue;
        }
        if !seen.insert(name) {
            anyhow::bail!("duplicate environment policy entry {name:?}");
        }
        match std::env::var(name) {
            Ok(value) => {
                vars.insert(name.clone(), value);
            }
            Err(_) if required => {
                anyhow::bail!("required host environment variable {name:?} is not set");
            }
            Err(_) => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_env_config_starts_empty() {
        let policy = EnvPolicy::from_config(EnvConfig::default()).unwrap();

        assert!(policy.vars().is_empty());
    }

    #[test]
    fn set_values_are_available_and_mutable() {
        let policy = EnvPolicy::from_config(EnvConfig {
            set: BTreeMap::from([("APP_ENV".to_owned(), "test".to_owned())]),
            ..EnvConfig::default()
        })
        .unwrap();

        assert_eq!(policy.get("APP_ENV").as_deref(), Some("test"));
        policy.set("APP_ENV".to_owned(), "dev".to_owned());
        assert_eq!(policy.get("APP_ENV").as_deref(), Some("dev"));
        policy.unset("APP_ENV");
        assert!(!policy.contains("APP_ENV"));
    }

    #[test]
    fn vars_are_returned_in_stable_name_order() {
        let policy = EnvPolicy::from_config(EnvConfig {
            set: BTreeMap::from([
                ("B".to_owned(), "2".to_owned()),
                ("A".to_owned(), "1".to_owned()),
            ]),
            ..EnvConfig::default()
        })
        .unwrap();

        assert_eq!(
            policy.vars(),
            vec![
                ("A".to_owned(), "1".to_owned()),
                ("B".to_owned(), "2".to_owned())
            ]
        );
    }

    #[test]
    fn duplicate_host_entries_are_an_error() {
        let error = EnvPolicy::from_config(EnvConfig {
            from_host: vec!["APP_ENV".to_owned(), "APP_ENV".to_owned()],
            ..EnvConfig::default()
        })
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("duplicate environment policy entry")
        );
    }

    #[test]
    fn missing_required_host_value_is_an_error() {
        let error = EnvPolicy::from_config(EnvConfig {
            required_from_host: vec!["MOONRUN_ENV_POLICY_TEST_MISSING".to_owned()],
            ..EnvConfig::default()
        })
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("MOONRUN_ENV_POLICY_TEST_MISSING")
        );
    }
}
