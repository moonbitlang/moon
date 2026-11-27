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

//! Child environment assembly shared by guest input preparation and native spawn.

use crate::upstream::ported_fns;

use std::ffi::{OsStr, OsString};

#[cfg(unix)]
#[derive(Debug, PartialEq, Eq)]
pub(super) enum LegacyEnvEntry {
    // Entries copied from the host environment are already materialized as
    // `key=value`. Guest-provided entries keep their boundary until the
    // legacy buffer is converted into the current builder.
    Materialized(OsString),
    Added { key: OsString, value: OsString },
}

#[cfg(unix)]
impl LegacyEnvEntry {
    fn into_materialized(self) -> Option<OsString> {
        match self {
            Self::Materialized(entry) => Some(entry),
            Self::Added { key, value } => unix_process_env_entry(key, value),
        }
    }
}

// Keep the inherited snapshot separate until spawn: native writes user entries
// first, then copies only inherited entries whose keys were not overridden.
pub(super) struct EnvBuilder {
    extra: Vec<OsString>,
    inherited: Vec<OsString>,
}

impl EnvBuilder {
    pub(super) fn new(inherited: Vec<OsString>) -> Self {
        Self {
            extra: Vec::new(),
            inherited,
        }
    }

    #[cfg(unix)]
    pub(super) fn from_legacy_env(
        mut entries: Vec<LegacyEnvEntry>,
        inherited_entry_count: usize,
    ) -> Self {
        let extra = entries.split_off(inherited_entry_count);
        Self {
            extra: extra
                .into_iter()
                .filter_map(LegacyEnvEntry::into_materialized)
                .collect(),
            inherited: entries
                .into_iter()
                .filter_map(LegacyEnvEntry::into_materialized)
                .collect(),
        }
    }
}

#[cfg(unix)]
pub(super) fn overwrite(env: &mut Vec<OsString>, key: &OsStr, value: &OsStr) {
    let mut builder = EnvBuilder::new(std::mem::take(env));
    builder.add_entry(key.to_owned(), value.to_owned());
    *env = builder.finish();
}

#[cfg(windows)]
pub(super) fn overwrite(env: &mut Vec<u16>, key: &OsStr, value: &OsStr) {
    use std::os::windows::ffi::OsStringExt;

    let inherited = env
        .split(|unit| *unit == 0)
        .take_while(|entry| !entry.is_empty())
        .map(OsString::from_wide)
        .collect();
    let mut builder = EnvBuilder::new(inherited);
    builder.add_entry(key.to_owned(), value.to_owned());
    *env = builder.finish();
}

#[cfg(unix)]
fn unix_process_env_entry(key: OsString, value: OsString) -> Option<OsString> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let key = key.as_os_str().as_bytes();
    if key.contains(&0) {
        return None;
    }
    let value = value.as_os_str().as_bytes();
    let value = &value[..value
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(value.len())];
    let mut entry = Vec::with_capacity(key.len() + value.len() + 1);
    entry.extend_from_slice(key);
    entry.push(b'=');
    entry.extend_from_slice(value);
    Some(OsString::from_vec(entry))
}

ported_fns! {
    impl EnvBuilder {
        #[ported(
            source = "src/process/unix.c",
            original = "moonbitlang_async_env_block_add_entry"
        )]
        #[cfg(unix)]
        pub(super) fn add_entry(
            &mut self,
            key: OsString,
            value: OsString,
        ) {
            if let Some(entry) = unix_process_env_entry(key, value) {
                self.extra.push(entry);
            }
        }

        #[ported(
            source = "src/process/windows.c",
            original = "moonbitlang_async_env_block_add_entry"
        )]
        #[cfg(windows)]
        pub(super) fn add_entry(
            &mut self,
            mut key: OsString,
            value: OsString,
        ) {
            use std::os::windows::ffi::{OsStrExt, OsStringExt};

            if key.encode_wide().any(|unit| unit == 0) {
                return;
            }
            let value = value
                .encode_wide()
                .take_while(|unit| *unit != 0)
                .collect::<Vec<_>>();
            key.push("=");
            key.push(OsString::from_wide(&value));
            self.extra.push(key);
        }

        #[ported(
            source = "src/process/unix.c",
            original = "moonbitlang_async_write_env_block"
        )]
        #[cfg(unix)]
        pub(super) fn finish(mut self) -> Vec<OsString> {
            use std::os::unix::ffi::OsStrExt;

            let extra_len = self.extra.len();
            let inherited = self
                .inherited
                .into_iter()
                .filter(|entry| {
                    let entry = entry.as_os_str().as_bytes();
                    let duplicate = if let Some(key_end) = entry.iter().position(|byte| *byte == b'=') {
                        self.extra[..extra_len].iter().any(|extra| {
                            extra
                                .as_os_str()
                                .as_bytes()
                                .get(..=key_end)
                                .is_some_and(|key| key == &entry[..=key_end])
                        })
                    } else {
                        self.extra[..extra_len]
                            .iter()
                            .any(|extra| extra.as_os_str().as_bytes() == entry)
                    };
                    !duplicate
                })
                .collect::<Vec<_>>();
            self.extra.extend(inherited);
            self.extra
        }

        #[ported(
            source = "src/process/windows.c",
            original = "moonbitlang_async_write_env_block"
        )]
        #[cfg(windows)]
        pub(super) fn finish(self) -> Vec<u16> {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};

            let extra_keys = self
                .extra
                .iter()
                .filter_map(|entry| {
                    let entry = entry.encode_wide().collect::<Vec<_>>();
                    let key_len = entry.iter().position(|unit| *unit == b'=' as u16)?;
                    Some(entry[..key_len].to_vec())
                })
                .collect::<Vec<_>>();
            let inherited = self
                .inherited
                .into_iter()
                .filter(|entry| {
                    let entry = entry.encode_wide().collect::<Vec<_>>();
                    let Some(key_len) = entry.iter().position(|unit| *unit == b'=' as u16) else {
                        return false;
                    };
                    if key_len == 0 {
                        return false;
                    }
                    !extra_keys.iter().any(|extra_key| {
                        key_len == extra_key.len()
                            && unsafe {
                                CompareStringOrdinal(
                                    extra_key.as_ptr(),
                                    extra_key.len() as i32,
                                    entry.as_ptr(),
                                    key_len as i32,
                                    1,
                                )
                            } == CSTR_EQUAL
                    })
                })
                .collect::<Vec<_>>();

            // Create the one contiguous block only when ownership moves to the
            // spawn job. Until this point the builder contains native OsStrings.
            let mut block = Vec::new();
            for entry in self.extra.into_iter().chain(inherited) {
                block.extend(entry.encode_wide());
                block.push(0);
            }
            if block.is_empty() {
                block.push(0);
            }
            block.push(0);
            block
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::ffi::OsStringExt;

    #[cfg(unix)]
    #[test]
    fn unix_process_env_discards_nul_keys_and_truncates_nul_values() {
        let mut builder = EnvBuilder::new(Vec::new());
        builder.add_entry(OsString::from("BAD\0INJECTED"), OsString::from("value"));
        builder.add_entry(
            OsString::from("GOOD"),
            OsString::from("before\0INJECTED=value"),
        );

        assert_eq!(builder.finish(), vec![OsString::from("GOOD=before")]);
    }

    #[cfg(unix)]
    #[test]
    fn unix_process_env_overwrite_replaces_guest_value_and_preserves_bytes() {
        let mut env = vec![
            OsString::from("MOONRUN_INHERITED_POLICY=guest"),
            OsString::from("KEEP=value"),
        ];
        let value = OsString::from_vec(b"/tmp/policy-\xFF.json".to_vec());

        overwrite(&mut env, OsStr::new("MOONRUN_INHERITED_POLICY"), &value);

        assert_eq!(
            env,
            vec![
                OsString::from_vec(b"MOONRUN_INHERITED_POLICY=/tmp/policy-\xFF.json".to_vec()),
                OsString::from("KEEP=value"),
            ]
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_process_env_discards_nul_keys_and_truncates_nul_values() {
        let mut builder = EnvBuilder::new(Vec::new());
        builder.add_entry(OsString::from("BAD\0INJECTED"), OsString::from("value"));
        builder.add_entry(
            OsString::from("GOOD"),
            OsString::from("before\0INJECTED=value"),
        );

        assert_eq!(
            builder.finish(),
            "GOOD=before\0\0".encode_utf16().collect::<Vec<_>>()
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_process_env_overwrite_replaces_guest_value_case_insensitively() {
        let mut env = "moonrun_inherited_policy=guest\0KEEP=\u{6708}\0\0"
            .encode_utf16()
            .collect();

        overwrite(
            &mut env,
            OsStr::new("MOONRUN_INHERITED_POLICY"),
            OsStr::new("C:\\\u{6708}\\policy.json"),
        );

        assert_eq!(
            env,
            "MOONRUN_INHERITED_POLICY=C:\\\u{6708}\\policy.json\0KEEP=\u{6708}\0\0"
                .encode_utf16()
                .collect::<Vec<_>>()
        );
    }

    #[cfg(unix)]
    #[test]
    fn process_env_builder_places_extra_entries_before_filtered_inherited_entries() {
        let mut builder = EnvBuilder::new(vec![
            OsString::from("OVERRIDE=old"),
            OsString::from("KEEP=value"),
            OsString::from("WITHOUT_SEPARATOR"),
        ]);
        builder.add_entry(OsString::from("OVERRIDE"), OsString::from("new"));

        assert_eq!(
            builder.finish(),
            vec![
                OsString::from("OVERRIDE=new"),
                OsString::from("KEEP=value"),
                OsString::from("WITHOUT_SEPARATOR"),
            ]
        );
    }

    #[cfg(windows)]
    #[test]
    fn process_env_builder_filters_inherited_entries_case_insensitively() {
        fn block(entries: &[&str]) -> Vec<u16> {
            let mut block = Vec::new();
            for entry in entries {
                block.extend(entry.encode_utf16());
                block.push(0);
            }
            block.push(0);
            block
        }

        let mut builder = EnvBuilder::new(vec![
            OsString::from("Path=old"),
            OsString::from("KEEP=value"),
            OsString::from("WITHOUT_SEPARATOR"),
            OsString::from("=PSEUDO"),
        ]);
        builder.add_entry(OsString::from("PATH"), OsString::from("new"));

        assert_eq!(builder.finish(), block(&["PATH=new", "KEEP=value"]));
    }

    #[test]
    fn ported_implementations_reference_upstream_sources() {
        for symbol in EnvBuilder::PORTED_SYMBOLS {
            symbol.assert_source_exists();
        }
    }
}
