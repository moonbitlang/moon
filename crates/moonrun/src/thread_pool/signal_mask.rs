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

//! Thread-affine signal-mask ownership for pool registration and Worker startup.

use std::io;
use std::marker::PhantomData;
use std::rc::Rc;

/// Restores the calling thread's mask on rollback, normal teardown, or unwind.
/// It must be dropped on that same thread, so it is neither Send nor Sync.
pub(crate) struct SignalMask {
    previous: libc::sigset_t,
    _thread: PhantomData<Rc<()>>,
}

impl SignalMask {
    pub(crate) fn for_thread_pool() -> io::Result<Self> {
        let mut signals = empty_signal_set()?;
        check_signal_call(unsafe { libc::sigaddset(&mut signals, libc::SIGCHLD) })?;
        let mut previous = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        check_pthread_call(unsafe {
            libc::pthread_sigmask(libc::SIG_BLOCK, &signals, &mut previous)
        })?;
        unsafe {
            libc::signal(libc::SIGPIPE, libc::SIG_IGN);
        }
        Ok(Self {
            previous,
            _thread: PhantomData,
        })
    }

    pub(super) fn for_worker_spawn() -> io::Result<Self> {
        let mut signals = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        check_signal_call(unsafe { libc::sigfillset(&mut signals) })?;
        let mut previous = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        check_pthread_call(unsafe {
            libc::pthread_sigmask(libc::SIG_SETMASK, &signals, &mut previous)
        })?;
        Ok(Self {
            previous,
            _thread: PhantomData,
        })
    }
}

impl Drop for SignalMask {
    fn drop(&mut self) {
        let _ = unsafe {
            libc::pthread_sigmask(libc::SIG_SETMASK, &self.previous, std::ptr::null_mut())
        };
    }
}

pub(super) fn unblock_worker_cancellation_signal() -> io::Result<()> {
    let mut signal = empty_signal_set()?;
    check_signal_call(unsafe { libc::sigaddset(&mut signal, libc::SIGUSR2) })?;
    check_pthread_call(unsafe {
        libc::pthread_sigmask(libc::SIG_UNBLOCK, &signal, std::ptr::null_mut())
    })
}

fn empty_signal_set() -> io::Result<libc::sigset_t> {
    let mut set = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    check_signal_call(unsafe { libc::sigemptyset(&mut set) })?;
    Ok(set)
}

fn check_signal_call(result: i32) -> io::Result<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn check_pthread_call(result: i32) -> io::Result<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_inherits_blocked_signals_and_parent_mask_is_restored() {
        let previous = crate::runtime::current_signal_mask().unwrap();
        let mask = SignalMask::for_worker_spawn().unwrap();
        let inherited = std::thread::spawn(|| {
            let inherited = crate::runtime::current_signal_mask().unwrap();
            unblock_worker_cancellation_signal().unwrap();
            let running = crate::runtime::current_signal_mask().unwrap();
            for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGCHLD] {
                assert_eq!(unsafe { libc::sigismember(&running, signal) }, 1);
            }
            assert_eq!(unsafe { libc::sigismember(&running, libc::SIGUSR2) }, 0);
            inherited
        });
        drop(mask);
        let restored = crate::runtime::current_signal_mask().unwrap();
        for signal in 1..32 {
            assert_eq!(unsafe { libc::sigismember(&restored, signal) }, unsafe {
                libc::sigismember(&previous, signal)
            });
        }
        let inherited = inherited.join().unwrap();
        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGCHLD, libc::SIGUSR2] {
            assert_eq!(unsafe { libc::sigismember(&inherited, signal) }, 1);
        }
    }

    #[test]
    fn mask_is_restored_when_registration_setup_unwinds() {
        let previous = crate::runtime::current_signal_mask().unwrap();
        let result = std::panic::catch_unwind(|| {
            let _mask = SignalMask::for_thread_pool().unwrap();
            panic!("registration setup interrupted");
        });
        assert!(result.is_err());
        let restored = crate::runtime::current_signal_mask().unwrap();
        for signal in 1..32 {
            assert_eq!(unsafe { libc::sigismember(&restored, signal) }, unsafe {
                libc::sigismember(&previous, signal)
            });
        }
    }
}
