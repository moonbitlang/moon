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

//! Environment state owned by one Runtime.
//!
//! Callers use one native-string interface regardless of whether the Run uses
//! moonrun's legacy ambient environment or an owned environment realized from
//! policy. The ambient backing deliberately preserves the existing write-
//! through contract: native executable lookup and native libraries can observe
//! the process environment without crossing this interface, so the embedder
//! must serialize process-environment access. An isolated backing must audit
//! those effects before replacing Ambient behavior.

use std::ffi::{OsStr, OsString};
use std::io;
use std::sync::RwLock;

use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum EnvError {
    #[error("invalid environment variable name {0:?}")]
    InvalidName(OsString),
    #[error("environment variable {0:?} contains a NUL in its value")]
    ValueContainsNul(OsString),
    #[error("failed to set environment variable {name:?}")]
    Set {
        name: OsString,
        #[source]
        source: io::Error,
    },
    #[error("failed to unset environment variable {name:?}")]
    Unset {
        name: OsString,
        #[source]
        source: io::Error,
    },
}

#[derive(Debug)]
pub(crate) struct Env {
    backing: EnvBacking,
}

#[derive(Debug)]
enum EnvBacking {
    Ambient,
    Owned(RwLock<Vec<(OsString, OsString)>>),
}

impl Env {
    pub(crate) fn ambient() -> Self {
        Self {
            backing: EnvBacking::Ambient,
        }
    }

    pub(crate) fn owned(
        entries: impl IntoIterator<Item = (OsString, OsString)>,
    ) -> Result<Self, EnvError> {
        let mut entries = entries.into_iter().collect::<Vec<_>>();
        for (name, value) in &entries {
            validate_initial_entry(name, value)?;
        }
        let mut normalized = Vec::with_capacity(entries.len());
        for (name, value) in entries.drain(..) {
            insert(&mut normalized, name, value);
        }
        Ok(Self {
            backing: EnvBacking::Owned(RwLock::new(normalized)),
        })
    }

    pub(crate) fn get(&self, name: &OsStr) -> Option<OsString> {
        if !os::valid_initial_name(name) {
            return None;
        }
        match &self.backing {
            EnvBacking::Ambient => std::env::var_os(name),
            EnvBacking::Owned(entries) => entries
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .iter()
                .find(|(stored, _)| os::names_equal(stored, name))
                .map(|(_, value)| value.clone()),
        }
    }

    pub(crate) fn set(&self, name: OsString, value: OsString) -> Result<(), EnvError> {
        validate_mutation(&name, &value)?;
        match &self.backing {
            EnvBacking::Ambient => {
                // This preserves moonrun's existing unrestricted behavior. A
                // later isolation change can replace this backing without
                // changing Env's interface or its consumers.
                // SAFETY: the legacy ambient contract requires the embedder to
                // serialize all process-environment access while a Run mutates it.
                unsafe { os::set(&name, &value) }.map_err(|source| EnvError::Set {
                    name: name.clone(),
                    source,
                })?;
            }
            EnvBacking::Owned(entries) => insert(
                &mut entries
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()),
                name,
                value,
            ),
        }
        Ok(())
    }

    pub(crate) fn unset(&self, name: &OsStr) -> Result<(), EnvError> {
        validate_name(name)?;
        match &self.backing {
            EnvBacking::Ambient => {
                // See the compatibility note in `set`.
                // SAFETY: the same legacy serialization contract applies here.
                unsafe { os::unset(name) }.map_err(|source| EnvError::Unset {
                    name: name.to_owned(),
                    source,
                })?;
            }
            EnvBacking::Owned(entries) => {
                let mut entries = entries
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Some(index) = entries
                    .iter()
                    .position(|(stored, _)| os::names_equal(stored, name))
                {
                    entries.remove(index);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn entries(&self) -> Vec<(OsString, OsString)> {
        match &self.backing {
            EnvBacking::Ambient => {
                // `vars_os` preserves native values. On Windows it is backed
                // by GetEnvironmentStringsW; failure to obtain the process
                // block is a process-wide condition from which no Run can
                // usefully recover. On Unix, Env deliberately models POSIX
                // `name=value` variables rather than opaque `envp` entries;
                // malformed raw entries such as `TOKEN` or `=x` are outside
                // this interface rather than being preserved byte-for-byte.
                std::env::vars_os().collect()
            }
            EnvBacking::Owned(entries) => entries
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone(),
        }
    }

    /// Entries inherited by a child created from this environment.
    ///
    /// This retains moonrun's existing Windows behavior: ambient drive-current
    /// pseudo entries are omitted, while an owned policy environment keeps the
    /// entries selected at startup. Process-block encoding stays in the process
    /// adapter.
    pub(crate) fn inherited_entries(&self) -> Vec<(OsString, OsString)> {
        match &self.backing {
            EnvBacking::Ambient => os::inherited_entries(),
            EnvBacking::Owned(_) => self.entries(),
        }
    }
}

fn insert(entries: &mut Vec<(OsString, OsString)>, name: OsString, value: OsString) {
    if let Some((stored_name, stored_value)) = entries
        .iter_mut()
        .find(|(stored, _)| os::names_equal(stored, &name))
    {
        *stored_name = name;
        *stored_value = value;
    } else {
        entries.push((name, value));
    }
    entries.sort_by(|(left, _), (right, _)| left.cmp(right));
}

fn validate_initial_entry(name: &OsStr, value: &OsStr) -> Result<(), EnvError> {
    if !os::valid_initial_name(name) {
        return Err(EnvError::InvalidName(name.to_owned()));
    }
    if os::contains_nul(value) {
        return Err(EnvError::ValueContainsNul(name.to_owned()));
    }
    Ok(())
}

fn validate_mutation(name: &OsStr, value: &OsStr) -> Result<(), EnvError> {
    validate_name(name)?;
    if os::contains_nul(value) {
        return Err(EnvError::ValueContainsNul(name.to_owned()));
    }
    Ok(())
}

fn validate_name(name: &OsStr) -> Result<(), EnvError> {
    if !os::valid_name(name) {
        return Err(EnvError::InvalidName(name.to_owned()));
    }
    Ok(())
}

#[cfg(unix)]
mod os {
    use super::*;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    pub(super) fn valid_initial_name(name: &OsStr) -> bool {
        if valid_name(name) {
            return true;
        }
        let name = name.as_bytes();
        name.first() == Some(&b'=') && name[1..].iter().all(|&byte| byte != b'=' && byte != 0)
    }

    pub(super) fn valid_name(name: &OsStr) -> bool {
        let name = name.as_bytes();
        !name.is_empty() && !name.contains(&b'=') && !name.contains(&0)
    }

    pub(super) fn contains_nul(value: &OsStr) -> bool {
        value.as_bytes().contains(&0)
    }

    pub(super) fn names_equal(left: &OsStr, right: &OsStr) -> bool {
        left == right
    }

    pub(super) fn inherited_entries() -> Vec<(OsString, OsString)> {
        std::env::vars_os().collect()
    }

    pub(super) unsafe fn set(name: &OsStr, value: &OsStr) -> io::Result<()> {
        let name = CString::new(name.as_bytes()).map_err(invalid_input)?;
        let value = CString::new(value.as_bytes()).map_err(invalid_input)?;
        if unsafe { libc::setenv(name.as_ptr(), value.as_ptr(), 1) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub(super) unsafe fn unset(name: &OsStr) -> io::Result<()> {
        let name = CString::new(name.as_bytes()).map_err(invalid_input)?;
        if unsafe { libc::unsetenv(name.as_ptr()) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn invalid_input(error: std::ffi::NulError) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidInput, error)
    }
}

#[cfg(windows)]
mod os {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
    use windows_sys::Win32::System::Environment::SetEnvironmentVariableW;

    pub(super) fn valid_initial_name(name: &OsStr) -> bool {
        let mut name = name.encode_wide();
        match name.next() {
            Some(first) if first == b'=' as u16 => {
                name.all(|unit| unit != b'=' as u16 && unit != 0)
            }
            Some(first) if first != 0 => name.all(|unit| unit != b'=' as u16 && unit != 0),
            _ => false,
        }
    }

    pub(super) fn valid_name(name: &OsStr) -> bool {
        let mut name = name.encode_wide();
        match name.next() {
            Some(first) if first != b'=' as u16 && first != 0 => {
                name.all(|unit| unit != b'=' as u16 && unit != 0)
            }
            _ => false,
        }
    }

    pub(super) fn contains_nul(value: &OsStr) -> bool {
        value.encode_wide().any(|unit| unit == 0)
    }

    pub(super) fn names_equal(left: &OsStr, right: &OsStr) -> bool {
        let left = left.encode_wide().collect::<Vec<_>>();
        let right = right.encode_wide().collect::<Vec<_>>();
        let (Ok(left_len), Ok(right_len)) = (i32::try_from(left.len()), i32::try_from(right.len()))
        else {
            return left == right;
        };
        (unsafe { CompareStringOrdinal(left.as_ptr(), left_len, right.as_ptr(), right_len, 1) })
            == CSTR_EQUAL
    }

    pub(super) fn inherited_entries() -> Vec<(OsString, OsString)> {
        std::env::vars_os()
            .filter(|(name, _)| name.encode_wide().next() != Some(b'=' as u16))
            .collect()
    }

    pub(super) unsafe fn set(name: &OsStr, value: &OsStr) -> io::Result<()> {
        let name = wide_null(name);
        let value = wide_null(value);
        if unsafe { SetEnvironmentVariableW(name.as_ptr(), value.as_ptr()) } != 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub(super) unsafe fn unset(name: &OsStr) -> io::Result<()> {
        let name = wide_null(name);
        if unsafe { SetEnvironmentVariableW(name.as_ptr(), std::ptr::null()) } != 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn wide_null(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(std::iter::once(0)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_environment_applies_mutations_over_initial_values() {
        let environment = Env::owned([
            ("KEEP".into(), "initial".into()),
            ("REMOVE".into(), "initial".into()),
        ])
        .unwrap();

        environment.set("KEEP".into(), "updated".into()).unwrap();
        environment.unset("REMOVE".as_ref()).unwrap();
        environment.set("ADD".into(), "new".into()).unwrap();

        assert_eq!(environment.get("KEEP".as_ref()), Some("updated".into()));
        assert_eq!(environment.get("REMOVE".as_ref()), None);
        assert_eq!(environment.get("ADD".as_ref()), Some("new".into()));
    }

    #[test]
    fn owned_environment_enumerates_its_current_values() {
        let environment =
            Env::owned([("B".into(), "initial".into()), ("A".into(), "one".into())]).unwrap();

        environment.set("B".into(), "two".into()).unwrap();
        environment.set("C".into(), "three".into()).unwrap();

        assert_eq!(
            environment.entries(),
            vec![
                ("A".into(), "one".into()),
                ("B".into(), "two".into()),
                ("C".into(), "three".into()),
            ]
        );
    }

    #[test]
    fn invalid_environment_entries_are_rejected_without_panicking() {
        assert!(Env::owned([("BAD=NAME".into(), "value".into())]).is_err());
        assert!(Env::owned([("NAME".into(), "bad\0value".into())]).is_err());

        let environment = Env::owned([("KEEP".into(), "value".into())]).unwrap();
        assert!(environment.set("BAD=NAME".into(), "value".into()).is_err());
        assert!(environment.set("NAME".into(), "bad\0value".into()).is_err());
        assert!(environment.unset("BAD=NAME".as_ref()).is_err());
        assert_eq!(environment.get("BAD=NAME".as_ref()), None);
        assert_eq!(environment.entries(), vec![("KEEP".into(), "value".into())]);
    }

    #[test]
    fn owned_environment_normalizes_duplicate_initial_names() {
        let environment = Env::owned([
            ("DUPLICATE".into(), "old".into()),
            ("DUPLICATE".into(), "new".into()),
        ])
        .unwrap();

        assert_eq!(
            environment.entries(),
            vec![("DUPLICATE".into(), "new".into())]
        );
    }

    #[test]
    fn ambient_environment_rejects_invalid_mutations_before_calling_the_os() {
        let environment = Env::ambient();

        assert!(environment.set("BAD=NAME".into(), "value".into()).is_err());
        assert!(environment.set("NAME".into(), "bad\0value".into()).is_err());
        assert!(environment.unset("BAD=NAME".as_ref()).is_err());
        assert_eq!(environment.get("BAD=NAME".as_ref()), None);
    }

    #[cfg(unix)]
    #[test]
    fn unix_environment_names_are_case_sensitive() {
        let environment = Env::owned([("Path".into(), "mixed".into())]).unwrap();

        assert_eq!(environment.get("Path".as_ref()), Some("mixed".into()));
        assert_eq!(environment.get("PATH".as_ref()), None);
    }

    #[cfg(windows)]
    #[test]
    fn windows_environment_names_are_case_insensitive() {
        let environment = Env::owned([("Path".into(), "old".into())]).unwrap();

        environment.set("PATH".into(), "new".into()).unwrap();

        assert_eq!(environment.get("path".as_ref()), Some("new".into()));
        assert_eq!(environment.entries(), vec![("PATH".into(), "new".into())]);
    }

    #[test]
    fn owned_environment_accepts_native_leading_equals_entries() {
        let environment = Env::owned([("=C:".into(), "native".into())]).unwrap();

        assert_eq!(environment.get("=C:".as_ref()), Some("native".into()));
        assert_eq!(environment.inherited_entries(), environment.entries());
        assert!(environment.set("=C:".into(), "changed".into()).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn windows_environment_adapter_reports_os_errors() {
        let result = unsafe { os::set("BAD=NAME".as_ref(), "value".as_ref()) };

        assert!(result.is_err());
    }
}
