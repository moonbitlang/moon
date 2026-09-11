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

use super::worker::WorkerCompletionId;
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
const CANCELLATION_ACKNOWLEDGED: i32 = 2;
const WAITING: i32 = 3;

#[derive(Debug)]
pub(super) struct WorkerCancellation {
    // The requesting thread marks Cancelling; the executing thread acknowledges
    // it and publishes Waiting only after the result. Assignment resets Running.
    // SeqCst orders these transitions with the region mark and cancellation
    // check; the later check-to-syscall window still requires signal retries.
    // The scheduling mutex separately tracks execution and its native target.
    state: AtomicI32,
    #[cfg(unix)]
    // Cancellation calls and Job assignment write this; the signal handler reads
    // it. The notifier is initialized before enabling retries. Disabling them
    // does not retract a retry already published or being published by a handler.
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
    pub(super) fn run<T>(
        &self,
        completion_id: WorkerCompletionId,
        operation: impl FnOnce() -> T,
    ) -> T {
        #[cfg(windows)]
        let _ = completion_id;
        let job = CurrentJob {
            cancellation: self,
            #[cfg(unix)]
            completion_id,
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
// is shared with the requesting thread; the completion ID stays immutable.
struct CurrentJob<'a> {
    cancellation: &'a WorkerCancellation,
    #[cfg(unix)]
    completion_id: WorkerCompletionId,
    // The executing thread marks syscall entry/exit; its Unix signal handler
    // reads the mark. Keep it atomic even though both run on the same thread.
    inside: AtomicBool,
}

// This private guard cannot escape WorkerCancellation::run. Its borrow keeps
// the stack context in place until TLS is cleared, including during unwinding.
struct CurrentJobBinding<'a> {
    // Lifetime-only borrow: TLS holds a raw pointer and cannot express this.
    _job: &'a CurrentJob<'a>,
}

impl<'a> CurrentJobBinding<'a> {
    fn enter(job: &'a CurrentJob<'a>) -> Self {
        assert!(
            current_job().is_null(),
            "a Worker cannot execute nested Jobs"
        );
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
        Self { _job: job }
    }
}

impl Drop for CurrentJobBinding<'_> {
    fn drop(&mut self) {
        // Clear TLS before the borrowed context can leave the stack.
        #[cfg(unix)]
        unsafe {
            libc::pthread_setspecific(*CURRENT_JOB_KEY.get().unwrap(), std::ptr::null());
        }
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::System::Threading::TlsSetValue(
                *CURRENT_JOB_KEY.get().unwrap(),
                std::ptr::null(),
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
}

impl Drop for CancellableRegion<'_> {
    fn drop(&mut self) {
        self.inside.store(false, Ordering::SeqCst);
    }
}

/// Run a cancellable syscall, or skip it if the Worker acknowledges cancellation.
/// Synchronous callers outside a Worker execute the operation normally.
/// An operation must not enter another cancellable region while this one is active.
pub(crate) fn with_cancellable_region<T>(
    operation: impl FnOnce() -> AsyncHostResult<T>,
) -> AsyncHostResult<T> {
    // A non-null pointer is installed only by WorkerCancellation::run, whose
    // borrowed context outlives this synchronous call. The region guard stays
    // private so neither it nor this reference can escape through the result.
    let Some(job) = (unsafe { current_job().as_ref() }) else {
        return operation();
    };
    assert!(
        !job.inside.swap(true, Ordering::SeqCst),
        "cancellable syscall regions cannot nest"
    );
    let _region = CancellableRegion {
        inside: &job.inside,
    };
    if job.cancellation.state.load(Ordering::SeqCst) == CANCELLING {
        job.cancellation
            .state
            .store(CANCELLATION_ACKNOWLEDGED, Ordering::SeqCst);
        #[cfg(unix)]
        return Err(AsyncHostError::Native(libc::EINTR));
        #[cfg(windows)]
        return Err(AsyncHostError::Native(
            windows_sys::Win32::Foundation::ERROR_OPERATION_ABORTED as i32,
        ));
    }
    operation()
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
        notifier.notify_from_signal(job.completion_id.as_i32());
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
            worker.run(WorkerCompletionId::from_abi(1), || {
                with_cancellable_region::<()>(|| {
                    panic!("unwind through the active cancellation region")
                })
                .unwrap();
            });
        });
        assert!(result.is_err());
        assert!(current_job().is_null());
        // Outside a Job, cancellation on this Worker cannot affect the call.
        assert!(worker.request());
        assert!(with_cancellable_region(|| Ok(())).is_ok());
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
            worker.run(WorkerCompletionId::from_abi(id), || {
                with_cancellable_region(|| {
                    worker.enable_retry(&notifier);
                    assert!(worker.request());
                    cancellation_signal_handler(libc::SIGUSR2);
                    Ok(())
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
        worker.run(WorkerCompletionId::from_abi(1), || {
            assert!(
                with_cancellable_region::<()>(|| panic!("cancelled syscall executed")).is_err()
            );
        });
        assert!(
            !worker.request(),
            "acknowledgement stops further cancellation attempts"
        );

        worker.finish();
        worker.start();
        assert_eq!(
            worker.run(WorkerCompletionId::from_abi(2), || with_cancellable_region(
                || Ok(42)
            )),
            Ok(42)
        );
        assert!(current_job().is_null());
        assert!(
            with_cancellable_region(|| Ok(())).is_ok(),
            "synchronous callers have no worker cancellation state"
        );
    }

    #[test]
    fn nested_job_scope_is_rejected_without_losing_the_current_job() {
        let worker = WorkerCancellation::new();
        worker.run(WorkerCompletionId::from_abi(1), || {
            let result = std::panic::catch_unwind(|| {
                worker.run(WorkerCompletionId::from_abi(2), || ());
            });
            assert!(result.is_err());
            assert!(worker.request());
            assert!(
                with_cancellable_region::<()>(|| panic!("current Job binding was lost")).is_err()
            );
        });
        assert!(current_job().is_null());
    }

    #[test]
    fn nested_region_is_rejected_without_clearing_the_active_region() {
        let worker = WorkerCancellation::new();
        worker.run(WorkerCompletionId::from_abi(1), || {
            with_cancellable_region(|| {
                // Both attempts must fail: rejecting the first must leave
                // the outer region marked until its own scope exits.
                for _ in 0..2 {
                    let result = std::panic::catch_unwind(|| with_cancellable_region(|| Ok(())));
                    assert!(result.is_err());
                }
                Ok(())
            })
            .unwrap();
            assert_eq!(with_cancellable_region(|| Ok(42)), Ok(42));
        });
    }

    #[test]
    fn operation_errors_leave_the_region_and_cancellation_skips_the_operation() {
        let worker = WorkerCancellation::new();
        worker.run(WorkerCompletionId::from_abi(1), || {
            assert_eq!(
                with_cancellable_region(|| Err::<(), _>(AsyncHostError::Fault)),
                Err(AsyncHostError::Fault)
            );
            assert_eq!(with_cancellable_region(|| Ok(42)), Ok(42));
            assert!(worker.request());
            assert!(
                with_cancellable_region::<()>(|| panic!("cancelled operation must not run"))
                    .is_err()
            );
        });
        assert_eq!(
            with_cancellable_region(|| Err::<(), _>(AsyncHostError::Fault)),
            Err(AsyncHostError::Fault)
        );
    }
}
