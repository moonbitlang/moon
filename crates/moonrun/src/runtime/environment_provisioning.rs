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

//! One-time construction input for a Runtime's owned environment.
//!
//! Host imports and configured values are resolved and normalized before the
//! Runtime creates its Env. This input is not consulted by runtime Env access.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;

use super::Env;

#[derive(Debug)]
pub(crate) struct EnvProvisioning {
    entries: Vec<(OsString, OsString)>,
}

impl EnvProvisioning {
    pub(crate) fn new(
        from_host: Vec<String>,
        required_from_host: Vec<String>,
        set: BTreeMap<String, String>,
    ) -> anyhow::Result<Self> {
        let mut vars = Vec::new();

        let copy_all = from_host.iter().any(|name| name == "*");
        if copy_all {
            vars.extend(std::env::vars_os());
        }

        copy_host_names(&mut vars, &from_host, false)?;
        copy_host_names(&mut vars, &required_from_host, true)?;

        for (name, value) in set {
            vars.push((name.into(), value.into()));
        }

        let environment = Env::owned(vars)?;
        Ok(Self {
            entries: environment.entries(),
        })
    }

    pub(crate) fn realize(self) -> anyhow::Result<Env> {
        Ok(Env::owned(self.entries)?)
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

    #[test]
    fn empty_provisioning_starts_empty() {
        let provisioning = EnvProvisioning::new(Vec::new(), Vec::new(), BTreeMap::new()).unwrap();

        assert!(provisioning.realize().unwrap().entries().is_empty());
    }

    #[test]
    fn set_values_are_applied_to_the_realized_environment() {
        let provisioning = EnvProvisioning::new(
            Vec::new(),
            Vec::new(),
            BTreeMap::from([("APP_ENV".to_owned(), "test".to_owned())]),
        )
        .unwrap();
        let environment = provisioning.realize().unwrap();

        assert_eq!(environment.get("APP_ENV".as_ref()), Some("test".into()));
        environment.set("APP_ENV".into(), "dev".into()).unwrap();
        assert_eq!(environment.get("APP_ENV".as_ref()), Some("dev".into()));
        environment.unset("APP_ENV".as_ref()).unwrap();
        assert_eq!(environment.get("APP_ENV".as_ref()), None);
    }

    #[test]
    fn vars_are_returned_in_stable_name_order() {
        let provisioning = EnvProvisioning::new(
            Vec::new(),
            Vec::new(),
            BTreeMap::from([
                ("B".to_owned(), "2".to_owned()),
                ("A".to_owned(), "1".to_owned()),
            ]),
        )
        .unwrap();

        assert_eq!(
            provisioning.realize().unwrap().entries(),
            vec![("A".into(), "1".into()), ("B".into(), "2".into())]
        );
    }

    #[test]
    fn duplicate_host_entries_are_an_error() {
        let error = EnvProvisioning::new(
            vec!["APP_ENV".to_owned(), "APP_ENV".to_owned()],
            Vec::new(),
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("duplicate environment policy entry")
        );
    }

    #[test]
    fn missing_required_host_value_is_an_error() {
        let error = EnvProvisioning::new(
            Vec::new(),
            vec!["MOONRUN_ENV_POLICY_TEST_MISSING".to_owned()],
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("MOONRUN_ENV_POLICY_TEST_MISSING")
        );
    }
}
