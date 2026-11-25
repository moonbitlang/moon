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

//! Process startup inputs and their transfer into spawn Jobs.
//!
//! These tables stay on the calling thread. Workers receive the completed
//! argument/environment values, never the Handles or mutable builders.

use std::cell::RefCell;
use std::ffi::OsString;
use std::rc::Rc;
use std::sync::Arc;

use slotmap::{Key, KeyData};

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::runtime::{Env, Handles, HostKeys, HostResourceKind as HandleKind};

use super::environment::EnvBuilder;
#[cfg(unix)]
use super::environment::LegacyEnvEntry;

#[cfg(unix)]
type LegacyEnv = Vec<Option<LegacyEnvEntry>>;
#[cfg(windows)]
type LegacyEnv = Vec<u16>;

pub(crate) struct ProcessInputs {
    environment: Arc<Env>,
    #[cfg(unix)]
    process_argvs: RefCell<Handles<Vec<Option<OsString>>>>,
    process_envs: RefCell<Handles<LegacyEnv>>,
    process_env_builders: RefCell<Handles<EnvBuilder>>,
}

impl ProcessInputs {
    pub(crate) fn new(environment: Arc<Env>, keys: Rc<RefCell<HostKeys>>) -> Self {
        Self {
            environment,
            #[cfg(unix)]
            process_argvs: RefCell::new(Handles::new(keys.clone(), HandleKind::ProcessArgv)),
            process_envs: RefCell::new(Handles::new(keys.clone(), HandleKind::ProcessEnv)),
            process_env_builders: RefCell::new(Handles::new(keys, HandleKind::ProcessEnvBuilder)),
        }
    }

    pub(crate) fn leak_summary(&self) -> Option<String> {
        let mut leaks = Vec::new();
        #[cfg(unix)]
        {
            let process_argvs = self.process_argvs.borrow();
            if !process_argvs.is_empty() {
                leaks.push(format!("process_argvs={}", process_argvs.len()));
            }
        }
        {
            let process_envs = self.process_envs.borrow();
            if !process_envs.is_empty() {
                leaks.push(format!("process_envs={}", process_envs.len()));
            }
        }
        {
            let process_env_builders = self.process_env_builders.borrow();
            if !process_env_builders.is_empty() {
                leaks.push(format!(
                    "process_env_builders={}",
                    process_env_builders.len()
                ));
            }
        }
        (!leaks.is_empty()).then(|| leaks.join(", "))
    }

    fn inherited_entries(&self) -> Vec<OsString> {
        self.environment
            .inherited_entries()
            .into_iter()
            .map(|(mut name, value)| {
                name.push("=");
                name.push(value);
                name
            })
            .collect()
    }

    pub(crate) fn make_env(&self, inherit: bool) -> u64 {
        let inherited = if inherit {
            self.inherited_entries()
        } else {
            Vec::new()
        };
        self.insert_env_builder(inherited)
    }

    pub(crate) fn capture_legacy_env(&self) -> u64 {
        let entries = self.inherited_entries();
        #[cfg(unix)]
        let env = entries.into_iter().map(Some).collect();
        #[cfg(windows)]
        let env = {
            use std::os::windows::ffi::OsStrExt;

            let mut block = Vec::new();
            for entry in entries {
                block.extend(entry.encode_wide());
                block.push(0);
            }
            if block.is_empty() {
                block.push(0);
            }
            block.push(0);
            block
        };
        self.insert_legacy_env(env)
    }

    pub(crate) fn allocate_legacy_env(&self, size: u32) -> AsyncHostResult<u64> {
        let size = usize::try_from(size).map_err(|_| AsyncHostError::Fault)?;
        #[cfg(unix)]
        let env = vec![None; size];
        #[cfg(windows)]
        let env = vec![0; size.checked_add(1).ok_or(AsyncHostError::Fault)?];
        Ok(self.insert_legacy_env(env))
    }

    #[cfg(unix)]
    pub(crate) fn allocate_argv(&self, len: u32) -> AsyncHostResult<u64> {
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        let key = self.process_argvs.borrow_mut().insert(vec![None; len]);
        Ok(key.data().as_ffi())
    }

    #[cfg(unix)]
    pub(crate) fn add_argv(&self, handle: u64, index: u32, value: OsString) -> AsyncHostResult<()> {
        let index = usize::try_from(index).map_err(|_| AsyncHostError::Fault)?;
        let key = KeyData::from_ffi(handle).into();
        let mut process_argvs = self.process_argvs.borrow_mut();
        let argv = process_argvs.get_mut(key).ok_or(AsyncHostError::Badf)?;
        let slot = argv.get_mut(index).ok_or(AsyncHostError::Fault)?;
        *slot = Some(value);
        Ok(())
    }

    #[cfg(unix)]
    pub(crate) fn take_legacy_spawn_inputs(
        &self,
        argv_handle: u64,
        env_handle: u64,
        inherited_env_entry_count: u32,
    ) -> AsyncHostResult<(Vec<OsString>, Vec<OsString>)> {
        let inherited_env_entry_count =
            usize::try_from(inherited_env_entry_count).map_err(|_| AsyncHostError::Inval)?;

        // Validate both buffers before consuming either handle. A malformed
        // spawn request must not partially transfer ownership.
        let argv_key = KeyData::from_ffi(argv_handle).into();
        let env_key = KeyData::from_ffi(env_handle).into();
        let mut process_argvs = self.process_argvs.borrow_mut();
        let argv = process_argvs.get(argv_key).ok_or(AsyncHostError::Badf)?;
        let mut process_envs = self.process_envs.borrow_mut();
        let env = process_envs.get(env_key).ok_or(AsyncHostError::Badf)?;
        if inherited_env_entry_count > env.len()
            || argv.iter().any(Option::is_none)
            || env.iter().any(Option::is_none)
        {
            return Err(AsyncHostError::Inval);
        }

        let argv = process_argvs
            .remove(argv_key)
            .ok_or(AsyncHostError::Badf)?
            .into_iter()
            .map(Option::unwrap)
            .collect();
        let env = process_envs
            .remove(env_key)
            .ok_or(AsyncHostError::Badf)?
            .into_iter()
            .map(Option::unwrap)
            .collect();
        // The legacy ABI materializes inherited entries first and appends
        // extras. Convert that layout into the current builder model so both
        // ABIs use the same override and ordering semantics from here on.
        let env = EnvBuilder::from_legacy_env(env, inherited_env_entry_count);
        let env = env.finish();
        Ok((argv, env))
    }

    #[cfg(unix)]
    pub(crate) fn take_spawn_inputs(
        &self,
        argv_handle: u64,
        env_handle: u64,
    ) -> AsyncHostResult<(Vec<OsString>, Vec<OsString>)> {
        // Validate both inputs before consuming either handle.
        let argv_key = KeyData::from_ffi(argv_handle).into();
        let env_key = KeyData::from_ffi(env_handle).into();
        let mut process_argvs = self.process_argvs.borrow_mut();
        let argv = process_argvs.get(argv_key).ok_or(AsyncHostError::Badf)?;
        let mut process_env_builders = self.process_env_builders.borrow_mut();
        process_env_builders
            .get(env_key)
            .ok_or(AsyncHostError::Badf)?;
        if argv.iter().any(Option::is_none) {
            return Err(AsyncHostError::Inval);
        }

        let argv = process_argvs
            .remove(argv_key)
            .ok_or(AsyncHostError::Badf)?
            .into_iter()
            .map(Option::unwrap)
            .collect();
        let env = process_env_builders
            .remove(env_key)
            .ok_or(AsyncHostError::Badf)?;
        let env = env.finish();
        Ok((argv, env))
    }

    #[cfg(unix)]
    fn insert_legacy_env(&self, entries: Vec<Option<OsString>>) -> u64 {
        let key = self.process_envs.borrow_mut().insert(
            entries
                .into_iter()
                .map(|entry| entry.map(LegacyEnvEntry::Materialized))
                .collect(),
        );
        key.data().as_ffi()
    }

    #[cfg(windows)]
    fn insert_legacy_env(&self, env: Vec<u16>) -> u64 {
        let key = self.process_envs.borrow_mut().insert(env);
        key.data().as_ffi()
    }

    fn insert_env_builder(&self, inherited: Vec<OsString>) -> u64 {
        let key = self
            .process_env_builders
            .borrow_mut()
            .insert(EnvBuilder::new(inherited));
        key.data().as_ffi()
    }

    pub(crate) fn add_env(
        &self,
        handle: u64,
        key: OsString,
        value: OsString,
    ) -> AsyncHostResult<()> {
        let builder_key = KeyData::from_ffi(handle).into();
        let mut builders = self.process_env_builders.borrow_mut();
        let builder = builders.get_mut(builder_key).ok_or(AsyncHostError::Badf)?;
        builder.add_entry(key, value);
        Ok(())
    }

    #[cfg(unix)]
    pub(crate) fn legacy_env_length(&self, handle: u64) -> AsyncHostResult<u32> {
        let key = KeyData::from_ffi(handle).into();
        let process_envs = self.process_envs.borrow();
        let env = process_envs.get(key).ok_or(AsyncHostError::Badf)?;
        u32::try_from(env.len()).map_err(|_| AsyncHostError::Fault)
    }

    #[cfg(windows)]
    pub(crate) fn legacy_env_length(&self, handle: u64) -> AsyncHostResult<u32> {
        let key = KeyData::from_ffi(handle).into();
        let process_envs = self.process_envs.borrow();
        let env = process_envs.get(key).ok_or(AsyncHostError::Badf)?;
        let len = env.len().checked_sub(1).ok_or(AsyncHostError::Fault)?;
        u32::try_from(len).map_err(|_| AsyncHostError::Fault)
    }

    #[cfg(unix)]
    pub(crate) fn transfer_legacy_env(
        &self,
        dst_handle: u64,
        src_handle: u64,
    ) -> AsyncHostResult<()> {
        if dst_handle == src_handle {
            return Err(AsyncHostError::Inval);
        }
        let dst_key = KeyData::from_ffi(dst_handle).into();
        if self.process_envs.borrow().get(dst_key).is_none() {
            return Err(AsyncHostError::Badf);
        }
        let src = self.take_legacy_env_buffer(src_handle)?;
        // The source is the temporary snapshot returned by get_curr_env.
        // Consume it here so its lifetime does not depend on deprecated free_env.
        let mut process_envs = self.process_envs.borrow_mut();
        let dst = process_envs.get_mut(dst_key).ok_or(AsyncHostError::Badf)?;
        if dst.len() < src.len() {
            return Err(AsyncHostError::Fault);
        }
        for (index, entry) in src.into_iter().enumerate() {
            dst[index] = entry;
        }
        Ok(())
    }

    #[cfg(windows)]
    pub(crate) fn transfer_legacy_env(
        &self,
        dst_handle: u64,
        src_handle: u64,
    ) -> AsyncHostResult<()> {
        if dst_handle == src_handle {
            return Err(AsyncHostError::Inval);
        }
        let dst_key = KeyData::from_ffi(dst_handle).into();
        if self.process_envs.borrow().get(dst_key).is_none() {
            return Err(AsyncHostError::Badf);
        }
        let src = self.take_legacy_env_buffer(src_handle)?;
        // The source is the temporary snapshot returned by get_curr_env.
        // Consume it here so its lifetime does not depend on deprecated free_env.
        let mut process_envs = self.process_envs.borrow_mut();
        let dst = process_envs.get_mut(dst_key).ok_or(AsyncHostError::Badf)?;
        let src_len = src.len().checked_sub(1).ok_or(AsyncHostError::Fault)?;
        if dst.len() <= src_len {
            return Err(AsyncHostError::Fault);
        }
        dst[..src_len].copy_from_slice(&src[..src_len]);
        Ok(())
    }

    #[cfg(unix)]
    pub(crate) fn add_legacy_env(
        &self,
        handle: u64,
        index: u32,
        key: OsString,
        value: OsString,
    ) -> AsyncHostResult<()> {
        let index = usize::try_from(index).map_err(|_| AsyncHostError::Fault)?;
        let env_key = KeyData::from_ffi(handle).into();
        let mut process_envs = self.process_envs.borrow_mut();
        let env = process_envs.get_mut(env_key).ok_or(AsyncHostError::Badf)?;
        let slot = env.get_mut(index).ok_or(AsyncHostError::Fault)?;
        *slot = Some(LegacyEnvEntry::Added { key, value });
        Ok(())
    }

    #[cfg(windows)]
    pub(crate) fn add_legacy_env(
        &self,
        handle: u64,
        offset: u32,
        key: &[u16],
        value: &[u16],
    ) -> AsyncHostResult<()> {
        let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
        let handle = KeyData::from_ffi(handle).into();
        let mut process_envs = self.process_envs.borrow_mut();
        let env = process_envs.get_mut(handle).ok_or(AsyncHostError::Badf)?;
        let value_start = offset
            .checked_add(key.len())
            .and_then(|index| index.checked_add(1))
            .ok_or(AsyncHostError::Fault)?;
        let end = value_start
            .checked_add(value.len())
            .and_then(|index| index.checked_add(1))
            .ok_or(AsyncHostError::Fault)?;
        if end > env.len() {
            return Err(AsyncHostError::Fault);
        }
        env[offset..offset + key.len()].copy_from_slice(key);
        env[offset + key.len()] = b'=' as u16;
        env[value_start..value_start + value.len()].copy_from_slice(value);
        env[value_start + value.len()] = 0;
        Ok(())
    }

    #[cfg(windows)]
    pub(crate) fn take_legacy_env(&self, handle: u64) -> AsyncHostResult<Vec<u16>> {
        self.take_legacy_env_buffer(handle)
    }

    #[cfg(windows)]
    pub(crate) fn take_env(&self, handle: u64) -> AsyncHostResult<Vec<u16>> {
        self.process_env_builders
            .borrow_mut()
            .remove(KeyData::from_ffi(handle).into())
            .map(EnvBuilder::finish)
            .ok_or(AsyncHostError::Badf)
    }

    fn take_legacy_env_buffer(&self, handle: u64) -> AsyncHostResult<LegacyEnv> {
        self.process_envs
            .borrow_mut()
            .remove(KeyData::from_ffi(handle).into())
            .ok_or(AsyncHostError::Badf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::null_handle;

    fn inputs() -> ProcessInputs {
        ProcessInputs::new(
            Arc::new(Env::owned([]).unwrap()),
            Rc::new(RefCell::new(HostKeys::default())),
        )
    }

    #[test]
    fn invalid_process_environment_handles_are_rejected() {
        let keys = Rc::new(RefCell::new(HostKeys::default()));
        let mut foreign = Handles::new(keys.clone(), HandleKind::CBuffer);
        let foreign_handle = foreign.insert(()).data().as_ffi();
        let host = ProcessInputs::new(Arc::new(Env::owned([]).unwrap()), keys);
        for handle in [0, null_handle(), foreign_handle] {
            assert_eq!(
                host.take_legacy_env_buffer(handle),
                Err(AsyncHostError::Badf)
            );
            #[cfg(windows)]
            assert_eq!(host.take_env(handle), Err(AsyncHostError::Badf));
        }
        assert!(host.leak_summary().is_none());
    }

    #[test]
    fn process_env_block_transfer_consumes_source() {
        let host = inputs();
        #[cfg(unix)]
        let src = host.insert_legacy_env(vec![Some(OsString::from("A=B"))]);
        #[cfg(windows)]
        let src = host.insert_legacy_env(vec![b'A' as u16, b'=' as u16, b'B' as u16, 0, 0]);
        #[cfg(unix)]
        let dst = host.insert_legacy_env(vec![None, None]);
        #[cfg(windows)]
        let dst = host.insert_legacy_env(vec![0; 7]);

        host.transfer_legacy_env(dst, src).unwrap();

        assert_eq!(host.legacy_env_length(src), Err(AsyncHostError::Badf));
        #[cfg(unix)]
        assert_eq!(
            host.take_legacy_env_buffer(dst).unwrap().as_slice(),
            &[
                Some(LegacyEnvEntry::Materialized(OsString::from("A=B"))),
                None,
            ]
        );
        #[cfg(windows)]
        assert_eq!(
            host.take_legacy_env_buffer(dst).unwrap().as_slice(),
            &[b'A' as u16, b'=' as u16, b'B' as u16, 0, 0, 0, 0]
        );
    }

    #[test]
    fn process_env_block_transfer_consumes_source_on_failure() {
        let host = inputs();
        #[cfg(unix)]
        let src = host.insert_legacy_env(vec![Some(OsString::from("A=B"))]);
        #[cfg(windows)]
        let src = host.insert_legacy_env(vec![b'A' as u16, b'=' as u16, b'B' as u16, 0, 0]);
        #[cfg(unix)]
        let dst = host.insert_legacy_env(vec![]);
        #[cfg(windows)]
        let dst = host.insert_legacy_env(vec![0]);

        assert_eq!(
            host.transfer_legacy_env(dst, src),
            Err(AsyncHostError::Fault)
        );
        assert_eq!(host.legacy_env_length(src), Err(AsyncHostError::Badf));
    }

    #[cfg(unix)]
    #[test]
    fn legacy_process_spawn_inputs_use_inherited_count_to_build_environment() {
        let host = inputs();
        let argv = host.allocate_argv(2).unwrap();
        host.add_argv(argv, 0, OsString::from("command")).unwrap();
        host.add_argv(argv, 1, OsString::from("argument")).unwrap();
        let env = host.insert_legacy_env(vec![
            Some(OsString::from("OVERRIDE=old")),
            Some(OsString::from("KEEP=value")),
            Some(OsString::from("OVERRIDE=new")),
        ]);

        let (args, entries) = host.take_legacy_spawn_inputs(argv, env, 2).unwrap();

        assert_eq!(
            args,
            vec![OsString::from("command"), OsString::from("argument")]
        );
        assert_eq!(
            entries,
            vec![OsString::from("OVERRIDE=new"), OsString::from("KEEP=value")]
        );
        assert_eq!(
            host.add_argv(argv, 0, OsString::from("command")),
            Err(AsyncHostError::Badf)
        );
        assert_eq!(host.legacy_env_length(env), Err(AsyncHostError::Badf));
    }

    #[cfg(unix)]
    #[test]
    fn legacy_process_spawn_inputs_apply_current_nul_handling() {
        let host = inputs();
        let argv = host.allocate_argv(1).unwrap();
        host.add_argv(argv, 0, OsString::from("command")).unwrap();
        let env = host.insert_legacy_env(vec![None, None]);
        host.add_legacy_env(
            env,
            0,
            OsString::from("BAD=PREFIX\0KEY"),
            OsString::from("value"),
        )
        .unwrap();
        host.add_legacy_env(
            env,
            1,
            OsString::from("GOOD"),
            OsString::from("before\0INJECTED=value"),
        )
        .unwrap();

        let (_, entries) = host.take_legacy_spawn_inputs(argv, env, 0).unwrap();

        assert_eq!(entries, vec![OsString::from("GOOD=before")]);
    }

    #[cfg(unix)]
    #[test]
    fn invalid_legacy_process_spawn_inherited_count_does_not_consume_buffers() {
        let host = inputs();
        let argv = host.allocate_argv(1).unwrap();
        host.add_argv(argv, 0, OsString::from("command")).unwrap();
        let env = host.insert_legacy_env(vec![Some(OsString::from("A=B"))]);

        assert_eq!(
            host.take_legacy_spawn_inputs(argv, env, 2),
            Err(AsyncHostError::Inval)
        );
        assert_eq!(
            host.take_legacy_spawn_inputs(argv, env, 1).unwrap(),
            (vec![OsString::from("command")], vec![OsString::from("A=B")])
        );
    }

    #[cfg(unix)]
    #[test]
    fn invalid_process_spawn_buffers_are_not_partially_consumed() {
        let host = inputs();
        let argv = host.allocate_argv(1).unwrap();
        host.add_argv(argv, 0, OsString::from("command")).unwrap();
        let env = host.insert_legacy_env(vec![None]);

        assert_eq!(
            host.take_legacy_spawn_inputs(argv, env, 0),
            Err(AsyncHostError::Inval)
        );
        host.add_legacy_env(env, 0, OsString::from("A"), OsString::from("B"))
            .unwrap();
        assert_eq!(
            host.take_legacy_spawn_inputs(argv, env, 0).unwrap(),
            (vec![OsString::from("command")], vec![OsString::from("A=B")])
        );
    }

    #[cfg(unix)]
    #[test]
    fn process_spawn_inputs_preserve_incomplete_argv_for_retry() {
        let host = inputs();
        let argv = host.allocate_argv(2).unwrap();
        host.add_argv(argv, 0, OsString::from("command")).unwrap();
        let env = host.insert_env_builder(vec![OsString::from("A=old")]);
        host.add_env(env, OsString::from("A"), OsString::from("new"))
            .unwrap();

        assert_eq!(
            host.take_spawn_inputs(argv, env),
            Err(AsyncHostError::Inval)
        );
        host.add_argv(argv, 1, OsString::from("argument")).unwrap();
        assert_eq!(
            host.take_spawn_inputs(argv, env).unwrap(),
            (
                vec![OsString::from("command"), OsString::from("argument")],
                vec![OsString::from("A=new")],
            )
        );
        assert_eq!(host.take_spawn_inputs(argv, env), Err(AsyncHostError::Badf));
        assert!(host.leak_summary().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn process_spawn_abis_reject_the_other_environment_handle_kind() {
        let host = inputs();
        let argv = host.allocate_argv(1).unwrap();
        host.add_argv(argv, 0, OsString::from("command")).unwrap();
        let builder = host.insert_env_builder(Vec::new());

        assert_eq!(
            host.take_legacy_spawn_inputs(argv, builder, 0),
            Err(AsyncHostError::Badf)
        );
        let legacy = host.insert_legacy_env(vec![Some(OsString::from("A=B"))]);
        assert_eq!(
            host.take_spawn_inputs(argv, legacy),
            Err(AsyncHostError::Badf)
        );
        assert_eq!(
            host.take_spawn_inputs(argv, builder).unwrap(),
            (vec![OsString::from("command")], Vec::new())
        );
        let argv = host.allocate_argv(0).unwrap();
        assert_eq!(
            host.take_legacy_spawn_inputs(argv, legacy, 0).unwrap(),
            (Vec::new(), vec![OsString::from("A=B")])
        );
        assert!(host.leak_summary().is_none());
    }

    #[cfg(windows)]
    #[test]
    fn process_spawn_environment_transfers_ownership() {
        let host = inputs();
        let block = vec![b'A' as u16, b'=' as u16, b'B' as u16, 0, 0];
        let env = host.insert_legacy_env(block.clone());

        assert_eq!(host.take_legacy_env(env).unwrap(), block);
        assert_eq!(host.legacy_env_length(env), Err(AsyncHostError::Badf));
    }

    #[cfg(windows)]
    #[test]
    fn process_spawn_abis_reject_the_other_environment_handle_kind() {
        let host = inputs();
        let builder = host.insert_env_builder(vec![OsString::from("A=B")]);
        assert_eq!(host.take_legacy_env(builder), Err(AsyncHostError::Badf));

        let legacy = host.insert_legacy_env(vec![0, 0]);
        assert_eq!(host.take_env(legacy), Err(AsyncHostError::Badf));
        assert_eq!(host.take_legacy_env(legacy).unwrap(), vec![0, 0]);
        assert_eq!(
            host.take_env(builder).unwrap(),
            vec![b'A' as u16, b'=' as u16, b'B' as u16, 0, 0]
        );
        let replacement = host.insert_env_builder(Vec::new());
        assert_ne!(builder, replacement);
        assert_eq!(host.take_env(builder), Err(AsyncHostError::Badf));
        host.take_env(replacement).unwrap();
        assert!(host.leak_summary().is_none());
    }
    #[test]
    fn environment_inheritance_is_captured_when_inputs_are_created() {
        let environment = Arc::new(Env::owned([("A".into(), "before".into())]).unwrap());
        let inputs = ProcessInputs::new(
            environment.clone(),
            Rc::new(RefCell::new(HostKeys::default())),
        );
        let inherited = inputs.make_env(true);
        let empty = inputs.make_env(false);
        let legacy = inputs.capture_legacy_env();
        environment.set("A".into(), "after".into()).unwrap();
        let later = inputs.make_env(true);

        #[cfg(unix)]
        {
            for (handle, expected) in [
                (inherited, vec![OsString::from("A=before")]),
                (empty, Vec::new()),
                (later, vec![OsString::from("A=after")]),
            ] {
                let argv = inputs.allocate_argv(0).unwrap();
                assert_eq!(inputs.take_spawn_inputs(argv, handle).unwrap().1, expected);
            }
            let argv = inputs.allocate_argv(0).unwrap();
            assert_eq!(
                inputs.take_legacy_spawn_inputs(argv, legacy, 1).unwrap().1,
                vec![OsString::from("A=before")]
            );
        }
        #[cfg(windows)]
        {
            for (handle, expected) in [
                (inherited, "A=before\0\0"),
                (empty, "\0\0"),
                (later, "A=after\0\0"),
            ] {
                assert_eq!(
                    inputs.take_env(handle).unwrap(),
                    expected.encode_utf16().collect::<Vec<_>>()
                );
            }
            assert_eq!(
                inputs.take_legacy_env(legacy).unwrap(),
                "A=before\0\0".encode_utf16().collect::<Vec<_>>()
            );
        }
        assert!(inputs.leak_summary().is_none());
    }
}
