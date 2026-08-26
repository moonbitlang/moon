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

use std::collections::BTreeSet;
use std::ffi::OsString;

use super::config::EnvConfig;
use crate::runtime::Env;

#[derive(Clone, Debug)]
pub(super) struct EnvPolicy {
    initial: Vec<(OsString, OsString)>,
}

impl EnvPolicy {
    pub(super) fn from_config(config: EnvConfig) -> anyhow::Result<Self> {
        let mut vars = Vec::new();

        let copy_all = config.from_host.iter().any(|name| name == "*");
        if copy_all {
            vars.extend(std::env::vars_os());
        }

        copy_host_names(&mut vars, &config.from_host, false)?;
        copy_host_names(&mut vars, &config.required_from_host, true)?;

        for (name, value) in config.set {
            vars.push((name.into(), value.into()));
        }

        let environment = Env::owned(vars)?;
        Ok(Self {
            initial: environment.entries(),
        })
    }

    pub(super) fn realize(&self) -> anyhow::Result<Env> {
        Ok(Env::owned(self.initial.clone())?)
    }
}

fn copy_host_names(
    vars: &mut Vec<(OsString, OsString)>,
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
        match std::env::var_os(name) {
            Some(value) => {
                vars.push((name.into(), value));
            }
            None if required => {
                anyhow::bail!("required host environment variable {name:?} is not set");
            }
            None => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn empty_env_config_starts_empty() {
        let policy = EnvPolicy::from_config(EnvConfig::default()).unwrap();

        assert!(policy.realize().unwrap().entries().is_empty());
    }

    #[test]
    fn set_values_are_applied_to_the_realized_environment() {
        let policy = EnvPolicy::from_config(EnvConfig {
            set: BTreeMap::from([("APP_ENV".to_owned(), "test".to_owned())]),
            ..EnvConfig::default()
        })
        .unwrap();
        let environment = policy.realize().unwrap();

        assert_eq!(environment.get("APP_ENV".as_ref()), Some("test".into()));
        environment.set("APP_ENV".into(), "dev".into()).unwrap();
        assert_eq!(environment.get("APP_ENV".as_ref()), Some("dev".into()));
        environment.unset("APP_ENV".as_ref()).unwrap();
        assert_eq!(environment.get("APP_ENV".as_ref()), None);
    }

    #[test]
    fn startup_policy_realizes_independent_environments() {
        let policy = EnvPolicy::from_config(EnvConfig {
            set: BTreeMap::from([("APP_ENV".to_owned(), "test".to_owned())]),
            ..EnvConfig::default()
        })
        .unwrap();

        let left = policy.realize().unwrap();
        let right = policy.realize().unwrap();
        left.set("APP_ENV".into(), "left".into()).unwrap();

        assert_eq!(left.get("APP_ENV".as_ref()), Some("left".into()));
        assert_eq!(right.get("APP_ENV".as_ref()), Some("test".into()));
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
            policy.realize().unwrap().entries(),
            vec![("A".into(), "1".into()), ("B".into(), "2".into())]
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
