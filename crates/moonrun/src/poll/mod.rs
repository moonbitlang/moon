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

//! Poll ownership, registrations, and the active event batch for one Runtime.

use std::cell::RefCell;
#[cfg(target_os = "macos")]
use std::collections::HashMap;
#[cfg(unix)]
use std::collections::HashSet;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
use std::rc::Rc;
use std::sync::Arc;

use slotmap::{Key, KeyData};

use crate::async_host::{AsyncHostError, AsyncHostResult};
#[cfg(unix)]
use crate::filesystem::RawFile as RawFd;
#[cfg(target_os = "macos")]
use crate::process::HostProcess;
#[cfg(windows)]
use crate::resource::Resource;
use crate::run_termination::TerminationRequest;
use crate::runtime::{Handles, HostKey, HostKeys, HostResourceKind};

mod os;

#[cfg(windows)]
pub(crate) use os::CompletionPort;

pub(crate) struct PollTable {
    polls: Handles<HostPoll>,
    current_event_poll: Option<HostKey>,
}

impl PollTable {
    pub(crate) fn new(keys: Rc<RefCell<HostKeys>>) -> Self {
        Self {
            polls: Handles::new(keys, HostResourceKind::Poll),
            current_event_poll: None,
        }
    }

    pub(crate) fn leak_summary(&self) -> Option<String> {
        let count = self.polls.len();
        (count != 0).then(|| format!("polls={count}"))
    }

    pub(crate) fn create(&mut self) -> AsyncHostResult<u64> {
        let instance = os::poll_create()?;
        let key = self.polls.insert(HostPoll {
            instance,
            termination_waker: None,
            #[cfg(windows)]
            io_completions: Vec::new(),
            #[cfg(target_os = "macos")]
            process_notifications: HashMap::new(),
            #[cfg(unix)]
            registered_fds: HashSet::new(),
        });
        Ok(key.data().as_ffi())
    }

    // Retire guest reachability first. The caller retains the detached Poll
    // until its thread-pool registration has been torn down.
    pub(crate) fn remove(&mut self, handle: u64) -> AsyncHostResult<HostPoll> {
        let key = KeyData::from_ffi(handle).into();
        let poll = self.polls.remove(key).ok_or(AsyncHostError::Badf)?;
        if self.current_event_poll == Some(key) {
            self.current_event_poll = None;
        }
        Ok(poll)
    }

    pub(crate) fn get_mut(&mut self, handle: u64) -> AsyncHostResult<&mut HostPoll> {
        self.polls
            .get_mut(KeyData::from_ffi(handle).into())
            .ok_or(AsyncHostError::Badf)
    }

    #[cfg(unix)]
    pub(crate) fn unregister_fd(&mut self, fd: RawFd) {
        for poll in self.polls.values_mut() {
            if poll.registered_fds.remove(&fd) {
                let _ = os::poll_unregister(&poll.instance, fd);
            }
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn register_pid(
        &mut self,
        poll_handle: u64,
        pid: i32,
        process: &HostProcess,
    ) -> AsyncHostResult<i32> {
        if let Some(context) = process.context() {
            let poll = self.get_mut(poll_handle)?;
            let result =
                context.observe_exit(pid, || match poll.process_notifications.entry(pid) {
                    std::collections::hash_map::Entry::Occupied(entry) => Ok(entry.get().clone()),
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        let notification =
                            os::register_user_notification(&poll.instance, pid as usize)?;
                        let wake = Arc::new(std::task::Waker::from(Arc::new(notification)));
                        entry.insert(wake.clone());
                        Ok(wake)
                    }
                });
            if result == Ok(0) {
                poll.process_notifications.remove(&pid);
            }
            return result;
        }
        process.with_owned_child_pid(pid, || {
            let poll = self.get_mut(poll_handle)?;
            Ok(os::poll_register_pid(&poll.instance, pid)?)
        })
    }

    // None means termination: no event batch is exposed and the caller must
    // leave completion restoration to Run teardown. On Windows, resolve native
    // I/O pointers once while publishing the batch, before guest access resumes.
    pub(crate) fn wait(
        &mut self,
        poll_handle: u64,
        timeout_ms: i32,
        termination: &TerminationRequest,
        #[cfg(windows)] mut complete_io: impl FnMut(
            *mut windows_sys::Win32::System::IO::OVERLAPPED,
        ) -> Option<HostKey>,
    ) -> AsyncHostResult<Option<i32>> {
        let poll_key = KeyData::from_ffi(poll_handle).into();
        let poll = self.polls.get_mut(poll_key).ok_or(AsyncHostError::Badf)?;
        let wait = termination.bind_host_wake(|| {
            if poll.termination_waker.is_none() {
                poll.termination_waker = Some(Arc::new(os::PollWaker::new(&poll.instance)?));
            }
            let waker = Arc::clone(poll.termination_waker.as_ref().unwrap());
            Ok::<_, AsyncHostError>(move || {
                if let Err(error) = waker.wake() {
                    tracing::error!(?error, "failed to wake a terminating Run's poll");
                }
            })
        })?;
        let result = os::poll_wait(&mut poll.instance, timeout_ms);
        #[cfg(windows)]
        {
            poll.io_completions = match result {
                Ok(count) => (0..count as usize)
                    .map(|index| {
                        let event =
                            os::event_list_get(&poll.instance, index).expect("dequeued event");
                        complete_io(os::event_get_io_result(event))
                    })
                    .collect(),
                Err(_) => Vec::new(),
            };
        }
        drop(wait);
        // The shared import adapter consumes the latched Run outcome. Never
        // publish the host-only wake event to the guest.
        if termination.is_requested() {
            self.current_event_poll = None;
            return Ok(None);
        }
        let count = result?;
        #[cfg(target_os = "macos")]
        for index in 0..count as usize {
            let event = os::event_list_get(&poll.instance, index).ok_or(AsyncHostError::Fault)?;
            if os::event_get_events(event) == os::PROCESS_EVENT {
                poll.process_notifications.remove(&os::event_get_pid(event));
            }
        }
        self.current_event_poll = Some(poll_key);
        Ok(Some(count))
    }

    pub(crate) fn get_event(&self, poll_handle: u64, index: u32) -> AsyncHostResult<u64> {
        let poll_key = KeyData::from_ffi(poll_handle).into();
        if self.current_event_poll != Some(poll_key) {
            return Err(AsyncHostError::Badf);
        }
        let poll = self.polls.get(poll_key).ok_or(AsyncHostError::Badf)?;
        os::event_list_get(&poll.instance, index as usize).ok_or(AsyncHostError::Fault)?;
        Ok(u64::from(index))
    }

    fn event(&self, handle: u64) -> AsyncHostResult<&os::PollEvent> {
        let index = u32::try_from(handle).map_err(|_| AsyncHostError::Fault)?;
        let key = self.current_event_poll.ok_or(AsyncHostError::Badf)?;
        let poll = self.polls.get(key).ok_or(AsyncHostError::Badf)?;
        os::event_list_get(&poll.instance, index as usize).ok_or(AsyncHostError::Fault)
    }

    pub(crate) fn event_fd(&self, handle: u64) -> AsyncHostResult<u64> {
        Ok(os::event_get_fd(self.event(handle)?))
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn event_pid(&self, handle: u64) -> AsyncHostResult<i32> {
        Ok(os::event_get_pid(self.event(handle)?))
    }

    #[cfg(unix)]
    pub(crate) fn event_events(&self, handle: u64) -> AsyncHostResult<i32> {
        Ok(os::event_get_events(self.event(handle)?))
    }

    #[cfg(windows)]
    pub(crate) fn event_io_result(&self, handle: u64) -> AsyncHostResult<HostKey> {
        let index = usize::try_from(handle).map_err(|_| AsyncHostError::Fault)?;
        let key = self.current_event_poll.ok_or(AsyncHostError::Badf)?;
        let poll = self.polls.get(key).ok_or(AsyncHostError::Badf)?;
        poll.io_completions
            .get(index)
            .ok_or(AsyncHostError::Fault)?
            .ok_or(AsyncHostError::Badf)
    }

    #[cfg(windows)]
    pub(crate) fn event_bytes_transferred(&self, handle: u64) -> AsyncHostResult<u32> {
        Ok(os::event_get_bytes_transferred(self.event(handle)?))
    }
}

#[derive(Debug)]
pub(crate) struct HostPoll {
    instance: os::PollInstance,
    termination_waker: Option<Arc<os::PollWaker>>,
    #[cfg(windows)]
    io_completions: Vec<Option<HostKey>>,
    // A registration belongs to this parent Run's poll, not to one guest
    // coroutine wait. Cancelling and retrying a wait must reuse its notifier.
    #[cfg(target_os = "macos")]
    process_notifications: HashMap<i32, Arc<std::task::Waker>>,
    #[cfg(unix)]
    registered_fds: HashSet<RawFd>,
}

impl HostPoll {
    pub(crate) fn destroy(self) {
        os::poll_destroy(self.instance);
    }

    #[cfg(unix)]
    pub(crate) fn register(
        &mut self,
        fd: RawFd,
        handle: u64,
        read_only: bool,
    ) -> AsyncHostResult<i32> {
        let user_data = usize::try_from(handle).map_err(|_| AsyncHostError::Fault)?;
        let registered = os::poll_register(&self.instance, fd, read_only, user_data)?;
        if registered {
            self.registered_fds.insert(fd);
        }
        Ok(i32::from(registered))
    }

    #[cfg(unix)]
    pub(crate) fn register_legacy(
        &mut self,
        fd: RawFd,
        handle: u64,
        read_only: bool,
    ) -> AsyncHostResult<()> {
        let user_data = usize::try_from(handle).map_err(|_| AsyncHostError::Fault)?;
        os::poll_register_legacy(&self.instance, fd, read_only, user_data)?;
        self.registered_fds.insert(fd);
        Ok(())
    }

    #[cfg(windows)]
    pub(crate) fn register(
        &mut self,
        resource: &Resource,
        handle: u64,
        read_only: bool,
    ) -> AsyncHostResult<i32> {
        let user_data = usize::try_from(handle).map_err(|_| AsyncHostError::Fault)?;
        let registered = if resource.resource_class().is_socket() {
            os::poll_register_socket(&self.instance, resource.as_socket()?, read_only, user_data)?
        } else {
            os::poll_register_file(
                &self.instance,
                resource.as_file()?.as_raw_handle(),
                read_only,
                user_data,
            )?
        };
        Ok(i32::from(registered))
    }

    #[cfg(windows)]
    pub(crate) fn completion_port(&self) -> CompletionPort {
        CompletionPort::from_poll(&self.instance)
    }

    #[cfg(unix)]
    pub(crate) fn register_completion_sources(
        &mut self,
        fds: [RawFd; 2],
        handle: u64,
    ) -> AsyncHostResult<()> {
        let user_data = usize::try_from(handle).map_err(|_| AsyncHostError::Fault)?;
        os::poll_register_completion_source(&self.instance, fds[0], user_data)?;
        if let Err(error) = os::poll_register_completion_source(&self.instance, fds[1], user_data) {
            let _ = os::poll_unregister(&self.instance, fds[0]);
            return Err(error.into());
        }
        self.registered_fds.extend(fds);
        Ok(())
    }
}

#[cfg(windows)]
pub(crate) fn post_thread_pool_completion(port: &CompletionPort, id: i32) -> AsyncHostResult<()> {
    // Native thread_pool.c encodes worker IDs as transferred bytes, with no
    // I/O request and INVALID_HANDLE_VALUE as the completion key.
    Ok(port.post(
        id as u32,
        windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE as usize,
    )?)
}

#[cfg(test)]
pub(crate) fn ported_symbols() -> Vec<crate::upstream::PortedSymbol> {
    os::ported_symbols()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finished_poll_wait_does_not_receive_later_termination() {
        let mut polls = PollTable::new(Default::default());
        let handle = polls.create().unwrap();
        let (terminator, receiver) = crate::termination_channel();
        assert_eq!(
            polls.wait(
                handle,
                0,
                &receiver.request,
                #[cfg(windows)]
                |_| None
            ),
            Ok(Some(0))
        );
        assert!(terminator.terminate(crate::RunTermination::Exit(7)));
        // Inspect without binding another wait. No stale sender may enqueue an
        // event after the previous wait has detached.
        let poll = polls.get_mut(handle).unwrap();
        assert_eq!(os::poll_wait(&mut poll.instance, 0).unwrap(), 0);
    }

    #[test]
    fn host_wakeup_stays_pending_until_poll_wait() {
        for terminate_before_binding in [true, false] {
            let (terminator, receiver) = crate::termination_channel();
            let mut poll = os::poll_create().unwrap();
            if terminate_before_binding {
                assert!(terminator.terminate(crate::RunTermination::Exit(7)));
            }
            let _wait = receiver
                .request
                .bind_host_wake(|| {
                    let waker = os::PollWaker::new(&poll)?;
                    Ok::<_, std::io::Error>(move || waker.wake().unwrap())
                })
                .unwrap();
            if !terminate_before_binding {
                assert!(terminator.terminate(crate::RunTermination::Exit(7)));
            }
            // Readiness must survive a request that precedes the syscall or
            // registration itself.
            assert_eq!(os::poll_wait(&mut poll, 0).unwrap(), 1);
        }
    }

    #[cfg(unix)]
    #[test]
    fn failed_completion_registration_leaves_no_partial_source() {
        use std::io::Write;
        use std::os::fd::AsRawFd;

        let mut polls = PollTable::new(Default::default());
        let handle = polls.create().unwrap();
        let (read, mut write) = std::io::pipe().unwrap();
        let (second, _write) = std::io::pipe().unwrap();
        let poll = polls.get_mut(handle).unwrap();
        assert!(
            poll.register_completion_sources([read.as_raw_fd(), -1], 42)
                .is_err()
        );
        write.write_all(b"x").unwrap();
        assert_eq!(polls.wait(handle, 0, &Default::default()), Ok(Some(0)));
        // Retrying the complete pair must neither see a duplicate registration
        // nor lose the readiness that arrived after the failed attempt.
        polls
            .get_mut(handle)
            .unwrap()
            .register_completion_sources([read.as_raw_fd(), second.as_raw_fd()], 42)
            .unwrap();
        assert_eq!(polls.wait(handle, 0, &Default::default()), Ok(Some(1)));
        let event = polls.get_event(handle, 0).unwrap();
        assert_eq!(polls.event_fd(event), Ok(42));
    }
}
