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

//! Native-shaped cancellation acknowledgement around potentially blocking calls.

use crate::async_host::{AsyncHostError, AsyncHostResult};
#[cfg(unix)]
use crate::async_sys::internal::event_loop::ThreadPoolCompletionNotifier;
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, AtomicI32, Ordering},
};

const RUNNING: i32 = 0;
const CANCELLING: i32 = 1;
const CANCELLED: i32 = 2;
const WAITING: i32 = 3;

#[derive(Debug)]
pub(super) struct WorkerCancellation {
    state: AtomicI32,
    inside: AtomicBool,
    #[cfg(unix)]
    job_id: AtomicI32,
    #[cfg(unix)]
    retry_enabled: AtomicBool,
    #[cfg(unix)]
    notifier: OnceLock<Arc<ThreadPoolCompletionNotifier>>,
}

impl WorkerCancellation {
    pub(super) fn new(id: i32) -> Self {
        #[cfg(windows)]
        let _ = id;
        Self {
            state: AtomicI32::new(RUNNING),
            inside: AtomicBool::new(false),
            #[cfg(unix)]
            job_id: AtomicI32::new(id),
            #[cfg(unix)]
            retry_enabled: AtomicBool::new(false),
            #[cfg(unix)]
            notifier: OnceLock::new(),
        }
    }

    pub(super) fn start(&self, id: i32) {
        #[cfg(windows)]
        let _ = id;
        self.inside.store(false, Ordering::SeqCst);
        #[cfg(unix)]
        {
            self.retry_enabled.store(false, Ordering::SeqCst);
            self.job_id.store(id, Ordering::SeqCst);
        }
        self.state.store(RUNNING, Ordering::SeqCst);
    }

    pub(super) fn finish(&self) {
        self.inside.store(false, Ordering::SeqCst);
        self.state.store(WAITING, Ordering::SeqCst);
    }

    pub(super) fn is_waiting(&self) -> bool {
        self.state.load(Ordering::SeqCst) == WAITING
    }

    /// False means the operation acknowledged cancellation or already finished.
    pub(super) fn request(&self) -> bool {
        match self
            .state
            .compare_exchange(RUNNING, CANCELLING, Ordering::SeqCst, Ordering::SeqCst)
        {
            Ok(_) | Err(CANCELLING) => true,
            Err(_) => false,
        }
    }

    #[cfg(unix)]
    pub(super) fn enable_retry(&self, notifier: &Arc<ThreadPoolCompletionNotifier>) {
        self.notifier.get_or_init(|| Arc::clone(notifier));
        self.retry_enabled.store(true, Ordering::SeqCst);
    }

    #[cfg(unix)]
    pub(super) fn disable_retry(&self) {
        self.retry_enabled.store(false, Ordering::SeqCst);
    }

    #[cfg(unix)]
    pub(super) fn retry_enabled(&self) -> bool {
        self.retry_enabled.load(Ordering::SeqCst)
    }
}

#[cfg(unix)]
static WORKER_KEY: OnceLock<libc::pthread_key_t> = OnceLock::new();
#[cfg(windows)]
static WORKER_KEY: OnceLock<u32> = OnceLock::new();

pub(super) struct CurrentWorker(Arc<WorkerCancellation>);

impl CurrentWorker {
    pub(super) fn enter(worker: Arc<WorkerCancellation>) -> Self {
        #[cfg(unix)]
        {
            let key = WORKER_KEY.get_or_init(|| {
                let mut key = 0;
                assert_eq!(unsafe { libc::pthread_key_create(&mut key, None) }, 0);
                key
            });
            assert_eq!(
                unsafe { libc::pthread_setspecific(*key, Arc::as_ptr(&worker).cast()) },
                0
            );
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Threading::{
                TLS_OUT_OF_INDEXES, TlsAlloc, TlsSetValue,
            };
            let key = WORKER_KEY.get_or_init(|| {
                let key = unsafe { TlsAlloc() };
                assert_ne!(key, TLS_OUT_OF_INDEXES);
                key
            });
            assert_ne!(unsafe { TlsSetValue(*key, Arc::as_ptr(&worker).cast()) }, 0);
        }
        Self(worker)
    }
}

impl Drop for CurrentWorker {
    fn drop(&mut self) {
        // Clear the TLS pointer before releasing the Arc that keeps it alive.
        #[cfg(unix)]
        unsafe {
            libc::pthread_setspecific(*WORKER_KEY.get().unwrap(), std::ptr::null());
        }
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::System::Threading::TlsSetValue(
                *WORKER_KEY.get().unwrap(),
                std::ptr::null(),
            );
        }
        let _ = &self.0;
    }
}

fn current_worker() -> *const WorkerCancellation {
    let Some(key) = WORKER_KEY.get() else {
        return std::ptr::null();
    };
    #[cfg(unix)]
    unsafe {
        libc::pthread_getspecific(*key).cast()
    }
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::System::Threading::TlsGetValue(*key).cast()
    }
}

/// Own the cancellation mark only for one cancellable syscall. Synchronous
/// callers outside a Worker keep their existing behavior.
pub(crate) struct CancellableRegion {
    worker: Option<Arc<WorkerCancellation>>,
    _same_thread: std::marker::PhantomData<*const ()>,
}

impl CancellableRegion {
    pub(crate) fn enter() -> AsyncHostResult<Self> {
        let raw = current_worker();
        let worker = if raw.is_null() {
            None
        } else {
            // CurrentWorker owns a strong reference until this thread leaves
            // its loop. Incrementing it keeps the region valid independently.
            Some(unsafe {
                Arc::increment_strong_count(raw);
                Arc::from_raw(raw)
            })
        };
        if let Some(worker) = &worker {
            worker.inside.store(true, Ordering::SeqCst);
            if worker.state.load(Ordering::SeqCst) == CANCELLING {
                worker.state.store(CANCELLED, Ordering::SeqCst);
                worker.inside.store(false, Ordering::SeqCst);
                #[cfg(unix)]
                return Err(AsyncHostError::Native(libc::EINTR));
                #[cfg(windows)]
                return Err(AsyncHostError::Native(
                    windows_sys::Win32::Foundation::ERROR_OPERATION_ABORTED as i32,
                ));
            }
        }
        Ok(Self {
            worker,
            _same_thread: std::marker::PhantomData,
        })
    }
}

impl Drop for CancellableRegion {
    fn drop(&mut self) {
        if let Some(worker) = &self.worker {
            worker.inside.store(false, Ordering::SeqCst);
        }
    }
}

#[cfg(unix)]
pub(super) extern "C" fn cancellation_signal_handler(_: i32) {
    // As in upstream #595, pthread_getspecific is signal-safe on the supported
    // glibc, musl, and macOS implementations, although POSIX does not require
    // it. Keep that platform dependency confined to this handler.
    #[cfg(target_os = "linux")]
    let errno = unsafe { libc::__errno_location() };
    #[cfg(target_os = "macos")]
    let errno = unsafe { libc::__error() };
    let saved_errno = unsafe { *errno };
    let worker = current_worker();
    if let Some(worker) = unsafe { worker.as_ref() }
        && worker.state.load(Ordering::SeqCst) == CANCELLING
        && worker.inside.load(Ordering::SeqCst)
        && worker.retry_enabled()
        && let Some(notifier) = worker.notifier.get()
    {
        // The signal may arrive before the syscall begins, so ask the guest
        // loop to retry until the worker acknowledges or publishes completion.
        notifier.notify_from_signal(worker.job_id.load(Ordering::SeqCst));
    }
    unsafe {
        *errno = saved_errno;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_before_syscall_is_acknowledged_and_reset_for_next_job() {
        let worker = Arc::new(WorkerCancellation::new(1));
        let current = CurrentWorker::enter(Arc::clone(&worker));
        assert!(worker.request());
        assert!(CancellableRegion::enter().is_err());
        assert!(
            !worker.request(),
            "acknowledgement stops further cancellation attempts"
        );
        assert!(!worker.inside.load(Ordering::SeqCst));

        worker.finish();
        worker.start(2);
        let region = CancellableRegion::enter().unwrap();
        assert!(worker.inside.load(Ordering::SeqCst));
        drop(region);
        assert!(!worker.inside.load(Ordering::SeqCst));
        drop(current);
        assert!(current_worker().is_null());
        assert!(
            CancellableRegion::enter().is_ok(),
            "synchronous callers have no worker cancellation state"
        );
    }
}
