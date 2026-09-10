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
use crate::async_sys::internal::event_loop::{
    CancellationRetryNotifier, ThreadPoolCompletionNotifier,
};
#[cfg(unix)]
use std::sync::Arc;
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, AtomicI32, Ordering},
};

const RUNNING: i32 = 0;
const CANCELLING: i32 = 1;
const CANCELLED: i32 = 2;
const WAITING: i32 = 3;

#[derive(Debug)]
pub(super) struct WorkerCancellation {
    // Status of the assigned Job. The Worker's scheduling mutex separately
    // tracks whether execution has begun and which native operation to cancel.
    state: AtomicI32,
    #[cfg(unix)]
    retry_enabled: AtomicBool,
    #[cfg(unix)]
    notifier: OnceLock<CancellationRetryNotifier>,
}

impl WorkerCancellation {
    pub(super) fn new() -> Self {
        Self {
            state: AtomicI32::new(RUNNING),
            #[cfg(unix)]
            retry_enabled: AtomicBool::new(false),
            #[cfg(unix)]
            notifier: OnceLock::new(),
        }
    }

    pub(super) fn start(&self) {
        #[cfg(unix)]
        {
            self.retry_enabled.store(false, Ordering::SeqCst);
            if let Some(notifier) = self.notifier.get() {
                notifier.reset();
            }
        }
        self.state.store(RUNNING, Ordering::SeqCst);
    }

    pub(super) fn finish(&self) {
        self.state.store(WAITING, Ordering::SeqCst);
    }

    /// Borrow the Worker's cancellation state for this Job's execution only.
    /// The TLS binding and all syscall regions end before the result is published.
    pub(super) fn run<T>(&self, id: i32, operation: impl FnOnce() -> T) -> T {
        #[cfg(windows)]
        let _ = id;
        let job = CurrentJob {
            cancellation: self,
            #[cfg(unix)]
            id,
            inside: AtomicBool::new(false),
        };
        let _binding = CurrentJobBinding::enter(&job);
        operation()
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
        self.notifier.get_or_init(|| notifier.cancellation_retry());
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
static CURRENT_JOB_KEY: OnceLock<libc::pthread_key_t> = OnceLock::new();
#[cfg(windows)]
static CURRENT_JOB_KEY: OnceLock<u32> = OnceLock::new();

// This context lives on the executing thread's stack. Only cancellation state
// is shared with the requesting thread; the Job ID stays immutable. The region
// mark is atomic because the signal handler can interrupt this same thread.
struct CurrentJob<'a> {
    cancellation: &'a WorkerCancellation,
    #[cfg(unix)]
    id: i32,
    inside: AtomicBool,
}

// This private guard cannot escape WorkerCancellation::run. Its borrow keeps
// the stack context in place until TLS is restored, including during unwinding.
struct CurrentJobBinding<'a> {
    _job: &'a CurrentJob<'a>,
    previous: *const CurrentJob<'a>,
}

impl<'a> CurrentJobBinding<'a> {
    fn enter(job: &'a CurrentJob<'a>) -> Self {
        let previous = current_job();
        #[cfg(unix)]
        {
            let key = CURRENT_JOB_KEY.get_or_init(|| {
                let mut key = 0;
                assert_eq!(unsafe { libc::pthread_key_create(&mut key, None) }, 0);
                key
            });
            assert_eq!(
                unsafe { libc::pthread_setspecific(*key, std::ptr::from_ref(job).cast()) },
                0
            );
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Threading::{
                TLS_OUT_OF_INDEXES, TlsAlloc, TlsSetValue,
            };
            let key = CURRENT_JOB_KEY.get_or_init(|| {
                let key = unsafe { TlsAlloc() };
                assert_ne!(key, TLS_OUT_OF_INDEXES);
                key
            });
            assert_ne!(
                unsafe { TlsSetValue(*key, std::ptr::from_ref(job).cast()) },
                0
            );
        }
        Self {
            _job: job,
            previous,
        }
    }
}

impl Drop for CurrentJobBinding<'_> {
    fn drop(&mut self) {
        // Restore TLS before the borrowed context can leave the stack.
        #[cfg(unix)]
        unsafe {
            libc::pthread_setspecific(*CURRENT_JOB_KEY.get().unwrap(), self.previous.cast());
        }
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::System::Threading::TlsSetValue(
                *CURRENT_JOB_KEY.get().unwrap(),
                self.previous.cast(),
            );
        }
    }
}

fn current_job<'a>() -> *const CurrentJob<'a> {
    let Some(key) = CURRENT_JOB_KEY.get() else {
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

struct CancellableRegion<'a> {
    inside: &'a AtomicBool,
    was_inside: bool,
}

impl Drop for CancellableRegion<'_> {
    fn drop(&mut self) {
        self.inside.store(self.was_inside, Ordering::SeqCst);
    }
}

/// Run a cancellable syscall, or skip it if the Worker acknowledges cancellation.
/// Synchronous callers outside a Worker execute the operation normally.
pub(crate) fn with_cancellable_region<T>(operation: impl FnOnce() -> T) -> AsyncHostResult<T> {
    // A non-null pointer is installed only by WorkerCancellation::run, whose
    // borrowed context outlives this synchronous call. The region guard stays
    // private so neither it nor this reference can escape through the result.
    let Some(job) = (unsafe { current_job().as_ref() }) else {
        return Ok(operation());
    };
    let _region = CancellableRegion {
        inside: &job.inside,
        was_inside: job.inside.swap(true, Ordering::SeqCst),
    };
    if job.cancellation.state.load(Ordering::SeqCst) == CANCELLING {
        job.cancellation.state.store(CANCELLED, Ordering::SeqCst);
        #[cfg(unix)]
        return Err(AsyncHostError::Native(libc::EINTR));
        #[cfg(windows)]
        return Err(AsyncHostError::Native(
            windows_sys::Win32::Foundation::ERROR_OPERATION_ABORTED as i32,
        ));
    }
    Ok(operation())
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
    // This handler runs on the interrupted thread, while the installed Job
    // context is still borrowed by WorkerCancellation::run.
    let job = current_job();
    if let Some(job) = unsafe { job.as_ref() }
        && job.cancellation.state.load(Ordering::SeqCst) == CANCELLING
        && job.inside.load(Ordering::SeqCst)
        && job.cancellation.retry_enabled()
        && let Some(notifier) = job.cancellation.notifier.get()
    {
        // The signal may arrive before the syscall begins. Record a retry in
        // this Worker's preallocated slot; publication never locks or waits
        // for the guest to drain the notification source.
        notifier.notify_from_signal(job.id);
    }
    unsafe {
        *errno = saved_errno;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_scope_is_cleared_during_unwind() {
        let worker = WorkerCancellation::new();
        let result = std::panic::catch_unwind(|| {
            worker.run(1, || {
                with_cancellable_region(|| panic!("unwind through the active cancellation region"))
                    .unwrap();
            });
        });
        assert!(result.is_err());
        assert!(current_job().is_null());
        assert_eq!(worker.state.load(Ordering::SeqCst), RUNNING);
        assert!(with_cancellable_region(|| ()).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn retry_notifications_follow_the_current_job_when_a_worker_is_reused() {
        use std::os::fd::{FromRawFd, OwnedFd};

        let (notifier, recv) = ThreadPoolCompletionNotifier::new().unwrap();
        let _recv = unsafe { OwnedFd::from_raw_fd(recv) };
        let notifier = Arc::new(notifier);
        let worker = WorkerCancellation::new();
        for id in [i32::MIN, i32::MAX] {
            worker.start();
            worker.run(id, || {
                with_cancellable_region(|| {
                    worker.enable_retry(&notifier);
                    assert!(worker.request());
                    cancellation_signal_handler(libc::SIGUSR2);
                })
                .unwrap();
            });

            let mut bytes = [0; 4];
            assert_eq!(notifier.fetch(&mut bytes).unwrap(), 4);
            assert_eq!(i32::from_ne_bytes(bytes), id);
            cancellation_signal_handler(libc::SIGUSR2);
            assert_eq!(notifier.fetch(&mut bytes).unwrap(), 0);
            worker.finish();
        }
    }

    #[test]
    fn cancellation_before_syscall_is_acknowledged_and_reset_for_next_job() {
        let worker = WorkerCancellation::new();
        assert!(worker.request());
        worker.run(1, || {
            assert!(with_cancellable_region(|| panic!("cancelled syscall executed")).is_err());
        });
        assert!(
            !worker.request(),
            "acknowledgement stops further cancellation attempts"
        );

        worker.finish();
        worker.start();
        assert_eq!(worker.run(2, || with_cancellable_region(|| 42)), Ok(42));
        assert!(current_job().is_null());
        assert!(
            with_cancellable_region(|| ()).is_ok(),
            "synchronous callers have no worker cancellation state"
        );
    }

    #[test]
    fn nested_job_and_region_scopes_restore_the_outer_context() {
        let outer = WorkerCancellation::new();
        let inner = WorkerCancellation::new();
        outer.run(1, || {
            with_cancellable_region(|| {
                let outer_job = current_job();
                let result = std::panic::catch_unwind(|| {
                    inner.run(2, || {
                        assert_ne!(current_job(), outer_job);
                        with_cancellable_region(|| panic!("unwind through the inner job")).unwrap();
                    });
                });
                assert!(result.is_err());
                assert_eq!(current_job(), outer_job);

                let result = std::panic::catch_unwind(|| {
                    with_cancellable_region(|| panic!("unwind through the inner region")).unwrap();
                });
                assert!(result.is_err());
                assert!(unsafe { (*outer_job).inside.load(Ordering::SeqCst) });
            })
            .unwrap();
            assert!(!unsafe { (*current_job()).inside.load(Ordering::SeqCst) });
        });
        assert!(current_job().is_null());
    }
}
