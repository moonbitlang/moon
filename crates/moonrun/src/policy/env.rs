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
use std::ffi::{OsStr, OsString};
use std::sync::{Arc, RwLock};

use super::config::EnvConfig;

/// The mutable environment visible to one Run.
///
/// Construction applies the startup-only policy once. Per-Run mutations stay
/// in `delta`; unmodified values are read lazily from the immutable source.
#[derive(Debug)]
pub(crate) struct Env {
    source: Arc<dyn EnvironmentSource>,
    delta: RwLock<EnvDelta>,
}

pub(crate) enum InitialEnv {
    Ambient,
    Explicit(Vec<(OsString, OsString)>),
}

#[derive(Clone, Debug)]
pub(super) struct EnvPolicy {
    from_host: Vec<String>,
    required_from_host: Vec<String>,
    set: BTreeMap<String, String>,
}

impl EnvPolicy {
    pub(super) fn from_config(config: EnvConfig) -> anyhow::Result<Self> {
        for (name, value) in &config.set {
            validate_environment_entry(OsStr::new(name), OsStr::new(value))?;
        }
        Ok(Self {
            from_host: validate_host_names(config.from_host)?,
            required_from_host: validate_host_names(config.required_from_host)?,
            set: config.set,
        })
    }

    fn selected_names(&self) -> Option<Vec<OsString>> {
        if self.from_host.iter().any(|name| name == "*") {
            return None;
        }

        let mut names: Vec<OsString> = Vec::new();
        for name in self
            .from_host
            .iter()
            .chain(&self.required_from_host)
            .filter(|name| name.as_str() != "*")
        {
            if !names
                .iter()
                .any(|selected| names_equal(selected, OsStr::new(name)))
            {
                names.push(name.into());
            }
        }
        Some(names)
    }

    fn realize(&self, source: Arc<dyn EnvironmentSource>) -> anyhow::Result<Env> {
        for name in self
            .required_from_host
            .iter()
            .filter(|name| name.as_str() != "*")
        {
            if source.get(OsStr::new(name)).is_none() {
                anyhow::bail!("required host environment variable {name:?} is not set");
            }
        }

        let source: Arc<dyn EnvironmentSource> = match self.selected_names() {
            Some(names) => Arc::new(SelectedEnvironment { source, names }),
            None => source,
        };
        let mut delta = EnvDelta::default();
        for (name, value) in &self.set {
            delta.set(name.into(), value.into());
        }
        Ok(Env::new(source, delta))
    }
}

impl Env {
    pub(super) fn realize(initial: InitialEnv, policy: Option<&EnvPolicy>) -> anyhow::Result<Self> {
        let source: Arc<dyn EnvironmentSource> = match initial {
            InitialEnv::Ambient => Arc::new(AmbientEnvironment),
            InitialEnv::Explicit(entries) => Arc::new(ExplicitEnvironment::new(entries)?),
        };
        Self::from_source(source, policy)
    }

    fn from_source(
        source: Arc<dyn EnvironmentSource>,
        policy: Option<&EnvPolicy>,
    ) -> anyhow::Result<Self> {
        match policy {
            Some(policy) => policy.realize(source),
            None => Ok(Self::new(source, EnvDelta::default())),
        }
    }

    fn new(source: Arc<dyn EnvironmentSource>, delta: EnvDelta) -> Self {
        Self {
            source,
            delta: RwLock::new(delta),
        }
    }

    /// Return the subset representable by MoonBit and WASI string APIs.
    ///
    /// Native process inheritance uses [`Self::process_entries`] instead, so
    /// an entry that is not Unicode is not lost from a child environment.
    pub(crate) fn utf8_vars(&self) -> Vec<(String, String)> {
        self.vars_os()
            .into_iter()
            .filter_map(|(name, value)| {
                Some((name.to_str()?.to_owned(), value.to_str()?.to_owned()))
            })
            .collect()
    }

    pub(crate) fn process_entries(&self) -> Vec<OsString> {
        self.vars_os()
            .into_iter()
            .map(|(name, value)| {
                let mut entry = name;
                entry.push("=");
                entry.push(&value);
                entry
            })
            .collect()
    }

    pub(crate) fn get(&self, name: &str) -> Option<String> {
        self.get_os(name)?.into_string().ok()
    }

    pub(crate) fn get_os(&self, name: &str) -> Option<OsString> {
        let change = self
            .delta
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(OsStr::new(name))
            .cloned();
        match change {
            Some(EnvChange::Set(value)) => Some(value),
            Some(EnvChange::Unset) => None,
            None => self.source.get(OsStr::new(name)).map(|(_, value)| value),
        }
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        self.get_os(name).is_some()
    }

    pub(crate) fn set(&self, name: String, value: String) {
        // Guest mutation imports cannot report configuration errors. Keep the
        // Env invariant without allowing an invalid update to panic the Run.
        if !valid_environment_name(OsStr::new(&name)) || contains_nul(OsStr::new(&value)) {
            return;
        }
        self.delta
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .set(name.into(), value.into());
    }

    pub(crate) fn unset(&self, name: &str) {
        if !valid_environment_name(OsStr::new(name)) {
            return;
        }
        self.delta
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .unset(name.into());
    }

    fn vars_os(&self) -> Vec<(OsString, OsString)> {
        let mut vars = self.source.vars();
        normalize(&mut vars);

        let delta = self
            .delta
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for (name, change) in &delta.changes {
            match change {
                EnvChange::Set(value) => insert(&mut vars, name.clone(), value.clone()),
                EnvChange::Unset => remove(&mut vars, name),
            }
        }
        vars
    }
}

trait EnvironmentSource: std::fmt::Debug + Send + Sync {
    fn get(&self, name: &OsStr) -> Option<(OsString, OsString)>;
    fn vars(&self) -> Vec<(OsString, OsString)>;
}

#[derive(Debug)]
struct AmbientEnvironment;

impl EnvironmentSource for AmbientEnvironment {
    fn get(&self, name: &OsStr) -> Option<(OsString, OsString)> {
        // `var_os` panics for invalid names; guest-provided lookup keys are
        // untrusted and one Run must not be able to unwind another.
        valid_environment_name(name)
            .then(|| std::env::var_os(name))
            .flatten()
            .map(|value| (name.to_os_string(), value))
    }

    fn vars(&self) -> Vec<(OsString, OsString)> {
        // `vars_os` preserves native OsString values and is backed by
        // GetEnvironmentStringsW on Windows. That implementation panics only
        // if the OS cannot return any environment block, a process-wide
        // failure from which no Run can usefully recover.
        std::env::vars_os().collect()
    }
}

#[cfg(unix)]
fn valid_environment_name(name: &OsStr) -> bool {
    use std::os::unix::ffi::OsStrExt;

    let name = name.as_bytes();
    !name.is_empty() && !name.contains(&b'=') && !name.contains(&0)
}

#[cfg(windows)]
fn valid_environment_name(name: &OsStr) -> bool {
    use std::os::windows::ffi::OsStrExt;

    let mut name = name.encode_wide().peekable();
    name.peek().is_some() && name.all(|unit| unit != b'=' as u16 && unit != 0)
}

#[derive(Debug)]
struct ExplicitEnvironment {
    vars: Vec<(OsString, OsString)>,
}

impl ExplicitEnvironment {
    fn new(mut vars: Vec<(OsString, OsString)>) -> anyhow::Result<Self> {
        for (name, value) in &vars {
            validate_environment_entry(name, value)?;
        }
        normalize(&mut vars);
        Ok(Self { vars })
    }
}

impl EnvironmentSource for ExplicitEnvironment {
    fn get(&self, name: &OsStr) -> Option<(OsString, OsString)> {
        self.vars
            .iter()
            .find(|(stored, _)| names_equal(stored, name))
            .cloned()
    }

    fn vars(&self) -> Vec<(OsString, OsString)> {
        self.vars.clone()
    }
}

#[derive(Debug)]
struct SelectedEnvironment {
    source: Arc<dyn EnvironmentSource>,
    names: Vec<OsString>,
}

impl EnvironmentSource for SelectedEnvironment {
    fn get(&self, name: &OsStr) -> Option<(OsString, OsString)> {
        self.names
            .iter()
            .any(|selected| names_equal(selected, name))
            .then(|| self.source.get(name))
            .flatten()
    }

    fn vars(&self) -> Vec<(OsString, OsString)> {
        self.names
            .iter()
            .filter_map(|name| self.source.get(name))
            .collect()
    }
}

#[derive(Debug, Default)]
struct EnvDelta {
    changes: Vec<(OsString, EnvChange)>,
}

impl EnvDelta {
    fn get(&self, name: &OsStr) -> Option<&EnvChange> {
        self.changes
            .iter()
            .find(|(stored, _)| names_equal(stored, name))
            .map(|(_, change)| change)
    }

    fn set(&mut self, name: OsString, value: OsString) {
        self.insert(name, EnvChange::Set(value));
    }

    fn unset(&mut self, name: OsString) {
        self.insert(name, EnvChange::Unset);
    }

    fn insert(&mut self, name: OsString, change: EnvChange) {
        if let Some((_, stored)) = self
            .changes
            .iter_mut()
            .find(|(stored, _)| names_equal(stored, &name))
        {
            *stored = change;
        } else {
            self.changes.push((name, change));
        }
    }
}

#[derive(Clone, Debug)]
enum EnvChange {
    Set(OsString),
    Unset,
}

fn validate_host_names(names: Vec<String>) -> anyhow::Result<Vec<String>> {
    let mut seen: Vec<&str> = Vec::new();
    for name in &names {
        if name != "*" && !valid_environment_name(OsStr::new(name)) {
            anyhow::bail!("invalid environment variable name {name:?}");
        }
        if name != "*"
            && seen
                .iter()
                .any(|seen| names_equal(OsStr::new(seen), OsStr::new(name)))
        {
            anyhow::bail!("duplicate environment policy entry {name:?}");
        }
        seen.push(name);
    }
    Ok(names)
}

fn validate_environment_entry(name: &OsStr, value: &OsStr) -> anyhow::Result<()> {
    if !valid_environment_name(name) {
        anyhow::bail!("invalid environment variable name {name:?}");
    }
    if contains_nul(value) {
        anyhow::bail!("environment variable {name:?} contains a NUL in its value");
    }
    Ok(())
}

#[cfg(unix)]
fn contains_nul(value: &OsStr) -> bool {
    use std::os::unix::ffi::OsStrExt;

    value.as_bytes().contains(&0)
}

#[cfg(windows)]
fn contains_nul(value: &OsStr) -> bool {
    use std::os::windows::ffi::OsStrExt;

    value.encode_wide().any(|unit| unit == 0)
}

fn normalize(vars: &mut Vec<(OsString, OsString)>) {
    let mut normalized = Vec::with_capacity(vars.len());
    for (name, value) in vars.drain(..) {
        insert(&mut normalized, name, value);
    }
    *vars = normalized;
}

fn insert(vars: &mut Vec<(OsString, OsString)>, name: OsString, value: OsString) {
    if let Some((_, stored_value)) = vars
        .iter_mut()
        .find(|(stored, _)| names_equal(stored, &name))
    {
        *stored_value = value;
    } else {
        vars.push((name, value));
    }
}

fn remove(vars: &mut Vec<(OsString, OsString)>, name: &OsStr) {
    if let Some(index) = vars
        .iter()
        .position(|(stored, _)| names_equal(stored, name))
    {
        vars.remove(index);
    }
}

#[cfg(not(windows))]
fn names_equal(left: &OsStr, right: &OsStr) -> bool {
    left == right
}

#[cfg(windows)]
fn names_equal(left: &OsStr, right: &OsStr) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};

    let left = left.encode_wide().collect::<Vec<_>>();
    let right = right.encode_wide().collect::<Vec<_>>();
    let (Ok(left_len), Ok(right_len)) = (i32::try_from(left.len()), i32::try_from(right.len()))
    else {
        return left == right;
    };
    (unsafe { CompareStringOrdinal(left.as_ptr(), left_len, right.as_ptr(), right_len, 1) })
        == CSTR_EQUAL
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct CountingEnvironment {
        vars: Vec<(OsString, OsString)>,
        gets: Arc<AtomicUsize>,
        enumerations: Arc<AtomicUsize>,
    }

    impl EnvironmentSource for CountingEnvironment {
        fn get(&self, name: &OsStr) -> Option<(OsString, OsString)> {
            self.gets.fetch_add(1, Ordering::Relaxed);
            self.vars
                .iter()
                .find(|(stored, _)| names_equal(stored, name))
                .cloned()
        }

        fn vars(&self) -> Vec<(OsString, OsString)> {
            self.enumerations.fetch_add(1, Ordering::Relaxed);
            self.vars.clone()
        }
    }

    fn counting_environment(
        policy: Option<&EnvPolicy>,
    ) -> (Env, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let gets = Arc::new(AtomicUsize::new(0));
        let enumerations = Arc::new(AtomicUsize::new(0));
        let source = CountingEnvironment {
            vars: vec![("SOURCE".into(), "value".into())],
            gets: Arc::clone(&gets),
            enumerations: Arc::clone(&enumerations),
        };
        (
            Env::from_source(Arc::new(source), policy).unwrap(),
            gets,
            enumerations,
        )
    }

    #[test]
    fn single_key_reads_and_delta_updates_do_not_enumerate_the_source() {
        let (environment, gets, enumerations) = counting_environment(None);

        assert_eq!(environment.get("SOURCE").as_deref(), Some("value"));
        environment.set("LOCAL".to_owned(), "set".to_owned());
        assert_eq!(environment.get("LOCAL").as_deref(), Some("set"));
        environment.unset("SOURCE");
        assert_eq!(environment.get("SOURCE"), None);

        assert_eq!(gets.load(Ordering::Relaxed), 1);
        assert_eq!(enumerations.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn materializing_all_vars_enumerates_the_source_once() {
        let (environment, _, enumerations) = counting_environment(None);

        assert_eq!(
            environment.utf8_vars(),
            vec![("SOURCE".to_owned(), "value".to_owned())]
        );

        assert_eq!(enumerations.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn invalid_ambient_names_do_not_panic() {
        let environment = Env::realize(InitialEnv::Ambient, None).unwrap();

        assert_eq!(environment.get(""), None);
        assert_eq!(environment.get("INVALID=NAME"), None);
        assert_eq!(environment.get("INVALID\0NAME"), None);
    }

    #[test]
    fn invalid_explicit_entries_are_rejected_at_run_construction() {
        for (name, value) in [("", "value"), ("BAD=NAME", "value"), ("NAME", "bad\0value")] {
            let result = Env::realize(
                InitialEnv::Explicit(vec![(name.into(), value.into())]),
                None,
            );
            assert!(result.is_err(), "accepted {name:?}={value:?}");
        }
    }

    #[test]
    fn invalid_policy_and_runtime_entries_are_not_installed() {
        assert!(
            EnvPolicy::from_config(EnvConfig {
                set: BTreeMap::from([("BAD=NAME".to_owned(), "value".to_owned())]),
                ..EnvConfig::default()
            })
            .is_err()
        );

        let environment = Env::realize(InitialEnv::Explicit(Vec::new()), None).unwrap();
        environment.set("BAD=NAME".to_owned(), "value".to_owned());
        environment.set("NAME".to_owned(), "bad\0value".to_owned());
        environment.unset("BAD=NAME");

        assert!(environment.utf8_vars().is_empty());
    }

    #[test]
    fn empty_policy_does_not_read_the_source_even_when_enumerated() {
        let policy = EnvPolicy::from_config(EnvConfig::default()).unwrap();
        let (environment, gets, enumerations) = counting_environment(Some(&policy));

        assert!(environment.utf8_vars().is_empty());

        assert_eq!(gets.load(Ordering::Relaxed), 0);
        assert_eq!(enumerations.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn selected_policy_materializes_only_selected_names() {
        let policy = EnvPolicy::from_config(EnvConfig {
            from_host: vec!["SOURCE".to_owned()],
            ..EnvConfig::default()
        })
        .unwrap();
        let (environment, gets, enumerations) = counting_environment(Some(&policy));

        assert_eq!(
            environment.utf8_vars(),
            vec![("SOURCE".to_owned(), "value".to_owned())]
        );

        assert_eq!(gets.load(Ordering::Relaxed), 1);
        assert_eq!(enumerations.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn empty_env_config_starts_empty() {
        let policy = EnvPolicy::from_config(EnvConfig::default()).unwrap();

        assert!(
            Env::realize(InitialEnv::Explicit(Vec::new()), Some(&policy))
                .unwrap()
                .utf8_vars()
                .is_empty()
        );
    }

    #[test]
    fn set_values_override_selected_inputs() {
        let policy = EnvPolicy::from_config(EnvConfig {
            set: BTreeMap::from([("APP_ENV".to_owned(), "test".to_owned())]),
            ..EnvConfig::default()
        })
        .unwrap();

        assert_eq!(
            Env::realize(
                InitialEnv::Explicit(vec![("APP_ENV".into(), "production".into())]),
                Some(&policy),
            )
            .unwrap()
            .get("APP_ENV")
            .as_deref(),
            Some("test")
        );
    }

    #[test]
    fn host_selection_reads_only_the_supplied_initial_environment() {
        let policy = EnvPolicy::from_config(EnvConfig {
            from_host: vec!["OPTIONAL".to_owned()],
            required_from_host: vec!["REQUIRED".to_owned()],
            ..EnvConfig::default()
        })
        .unwrap();
        let initial = vec![
            ("OPTIONAL".into(), "one".into()),
            ("REQUIRED".into(), "two".into()),
            ("HIDDEN".into(), "secret".into()),
        ];

        assert_eq!(
            Env::realize(InitialEnv::Explicit(initial), Some(&policy))
                .unwrap()
                .utf8_vars(),
            vec![
                ("OPTIONAL".to_owned(), "one".to_owned()),
                ("REQUIRED".to_owned(), "two".to_owned()),
            ]
        );
    }

    #[test]
    fn wildcard_copies_the_supplied_initial_environment() {
        let policy = EnvPolicy::from_config(EnvConfig {
            from_host: vec!["*".to_owned()],
            ..EnvConfig::default()
        })
        .unwrap();
        let initial = vec![("A".into(), "1".into()), ("B".into(), "2".into())];

        assert_eq!(
            Env::realize(InitialEnv::Explicit(initial), Some(&policy))
                .unwrap()
                .utf8_vars(),
            vec![
                ("A".to_owned(), "1".to_owned()),
                ("B".to_owned(), "2".to_owned()),
            ]
        );
    }

    #[test]
    fn selected_vars_are_returned_in_stable_name_order() {
        let policy = EnvPolicy::from_config(EnvConfig {
            set: BTreeMap::from([
                ("B".to_owned(), "2".to_owned()),
                ("A".to_owned(), "1".to_owned()),
            ]),
            ..EnvConfig::default()
        })
        .unwrap();

        assert_eq!(
            Env::realize(InitialEnv::Explicit(Vec::new()), Some(&policy))
                .unwrap()
                .utf8_vars(),
            vec![
                ("A".to_owned(), "1".to_owned()),
                ("B".to_owned(), "2".to_owned()),
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
        let policy = EnvPolicy::from_config(EnvConfig {
            required_from_host: vec!["MOONRUN_ENV_POLICY_TEST_MISSING".to_owned()],
            ..EnvConfig::default()
        })
        .unwrap();
        let error = Env::realize(InitialEnv::Explicit(Vec::new()), Some(&policy)).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("MOONRUN_ENV_POLICY_TEST_MISSING")
        );
    }

    #[test]
    fn run_environments_are_independent_after_policy_construction() {
        let policy = EnvPolicy::from_config(EnvConfig {
            from_host: vec!["SHARED".to_owned()],
            ..EnvConfig::default()
        })
        .unwrap();
        let initial = vec![("SHARED".into(), "initial".into())];
        let left = Env::realize(InitialEnv::Explicit(initial.clone()), Some(&policy)).unwrap();
        let right = Env::realize(InitialEnv::Explicit(initial), Some(&policy)).unwrap();

        left.set("SHARED".to_owned(), "left".to_owned());
        right.unset("SHARED");

        assert_eq!(left.get("SHARED").as_deref(), Some("left"));
        assert!(!right.contains("SHARED"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_environment_names_are_case_insensitive() {
        let environment = Env::realize(
            InitialEnv::Explicit(vec![("Path".into(), "initial".into())]),
            None,
        )
        .unwrap();

        environment.set("PATH".to_owned(), "updated".to_owned());

        assert_eq!(environment.get("path").as_deref(), Some("updated"));
        assert_eq!(environment.utf8_vars().len(), 1);
        environment.unset("pAtH");
        assert!(!environment.contains("PATH"));
    }

    #[cfg(unix)]
    #[test]
    fn native_process_entries_preserve_non_unicode_values_and_order() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let environment = Env::realize(
            InitialEnv::Explicit(vec![
                ("SECOND".into(), OsString::from_vec(vec![b'v', 0xff])),
                ("FIRST".into(), "one".into()),
            ]),
            None,
        )
        .unwrap();

        assert!(environment.contains("SECOND"));
        assert_eq!(environment.get("SECOND"), None);
        assert_eq!(
            environment.utf8_vars(),
            vec![("FIRST".to_owned(), "one".to_owned())]
        );
        assert_eq!(
            environment
                .process_entries()
                .into_iter()
                .map(|entry| entry.as_os_str().as_bytes().to_vec())
                .collect::<Vec<_>>(),
            vec![b"SECOND=v\xff".to_vec(), b"FIRST=one".to_vec()]
        );
    }

    #[test]
    fn poisoned_lock_does_not_make_the_environment_panic() {
        let environment = Env::realize(
            InitialEnv::Explicit(vec![("KEY".into(), "before".into())]),
            None,
        )
        .unwrap();
        let _ = std::panic::catch_unwind(|| {
            let _guard = environment.delta.write().unwrap();
            panic!("poison the test lock");
        });

        environment.set("KEY".to_owned(), "after".to_owned());

        assert_eq!(environment.get("KEY").as_deref(), Some("after"));
    }
}
