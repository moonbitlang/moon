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

use std::sync::{Arc, Mutex};

/// The outcome requested when ending one Run, without terminating the host process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunTermination {
    /// Record an exit status, including a caller-selected Windows termination status.
    Exit(i32),
    /// Record termination by a signal. This does not send an operating-system signal.
    KilledBySignal(i32),
}

/// Thread-safe control for forcibly ending the Run bound to its receiver.
///
/// Requests are retained before execution starts. Dropping a terminator does
/// not stop the Run; after the receiver is dropped, this handle is disconnected.
#[derive(Clone)]
pub struct RunTerminator {
    request: TerminationRequest,
}

/// The single-use receiving half of a Run termination channel.
/// Pass it to [`crate::RunOptions::with_termination_receiver`].
/// It closes when the Run returns (including errors) or the receiver is dropped.
pub struct RunTerminationReceiver {
    pub(crate) request: TerminationRequest,
}

/// Create a termination channel for exactly one Run.
pub fn termination_channel() -> (RunTerminator, RunTerminationReceiver) {
    let request = TerminationRequest::default();
    request.0.lock().unwrap().connected = true;
    (
        RunTerminator {
            request: request.clone(),
        },
        RunTerminationReceiver { request },
    )
}

impl RunTerminator {
    pub(crate) fn requested(&self) -> Option<RunTermination> {
        self.request.get()
    }

    /// Record the first termination request and interrupt guest execution.
    ///
    /// `true` means this request was accepted, not that the Run has finished.
    /// `false` means an outcome was already requested or the receiver is closed.
    /// This provides no grace period and cannot forcibly unwind a synchronous
    /// host operation. Such an operation must return before the Run can finish.
    pub fn terminate(&self, outcome: RunTermination) -> bool {
        // FIXME: Add operation-specific cancellation for synchronous host I/O.
        // A blocked call can delay teardown indefinitely; never forcibly unwind
        // Rust/V8 or report completion before the call and teardown finish.
        let mut state = self.request.0.lock().unwrap();
        if !state.connected || state.outcome.is_some() {
            return false;
        }
        state.outcome = Some(outcome);
        // Host waits must return even while their callback defers V8 interruption.
        if let Some(wake) = &state.host_wake {
            wake();
        }
        if state.suspended == 0
            && let Some(interrupt) = &state.interrupt
        {
            interrupt();
        }
        true
    }
}

impl RunTerminationReceiver {
    pub(crate) fn sender(&self) -> RunTerminator {
        RunTerminator {
            request: self.request.clone(),
        }
    }

    pub(crate) fn close(&self) -> Option<RunTermination> {
        self.request.close()
    }
}

impl Drop for RunTerminationReceiver {
    fn drop(&mut self) {
        self.close();
    }
}

impl std::fmt::Debug for RunTerminationReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunTerminationReceiver")
            .finish_non_exhaustive()
    }
}

struct State {
    outcome: Option<RunTermination>,
    connected: bool,
    interrupt: Option<Box<dyn Fn() + Send + Sync>>,
    host_wake: Option<Box<dyn Fn() + Send + Sync>>,
    suspended: usize,
}

// The same outcome is shared by guest exit imports and external requests.
// It stays latched until Run completion; reading it never admits a later exit.
#[derive(Clone)]
pub(crate) struct TerminationRequest(Arc<Mutex<State>>);

impl Default for TerminationRequest {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(State {
            outcome: None,
            connected: false,
            interrupt: None,
            host_wake: None,
            suspended: 0,
        })))
    }
}

impl TerminationRequest {
    // Only the receiver owner finalizes execution. Borrowers may request an
    // outcome or bind interruption, but cannot shorten the surrounding scope.
    fn close(&self) -> Option<RunTermination> {
        let mut state = self.0.lock().unwrap();
        state.connected = false;
        state.interrupt = None;
        state.host_wake = None;
        state.outcome
    }

    pub(crate) fn request(&self, termination: RunTermination) {
        // Imports record their outcome while Guest Memory may still be borrowed.
        // Their adapter interrupts only after the host call has returned.
        self.0.lock().unwrap().outcome.get_or_insert(termination);
    }

    pub(crate) fn is_requested(&self) -> bool {
        self.get().is_some()
    }

    pub(crate) fn get(&self) -> Option<RunTermination> {
        self.0.lock().unwrap().outcome
    }

    #[cfg(feature = "v8")]
    pub(crate) fn is_connected(&self) -> bool {
        self.0.lock().unwrap().connected
    }

    /// Register one synchronous host wait. Without an external receiver, skip
    /// creation entirely. Creation and binding share the termination lock so an
    /// early request cannot be missed; neither callback may re-enter this state.
    /// Wakeups must be nonblocking and remain pending until observed.
    pub(crate) fn bind_host_wake<W, E>(
        &self,
        create_wake: impl FnOnce() -> Result<W, E>,
    ) -> Result<Option<HostWaitGuard>, E>
    where
        W: Fn() + Send + Sync + 'static,
    {
        let mut state = self.0.lock().unwrap();
        if !state.connected {
            return Ok(None);
        }
        assert!(state.host_wake.is_none());
        state.host_wake = Some(Box::new(create_wake()?));
        if state.outcome.is_some() {
            state.host_wake.as_ref().unwrap()();
        }
        Ok(Some(HostWaitGuard(self.clone())))
    }

    /// Bind only while engine interruption is safe. The callback must not
    /// re-enter this state: it runs under the lock to serialize with unbinding.
    #[cfg(feature = "v8")]
    pub(crate) fn bind_interrupt(
        &self,
        interrupt: impl Fn() + Send + Sync + 'static,
    ) -> Option<InterruptGuard> {
        let mut state = self.0.lock().unwrap();
        if !state.connected {
            return None;
        }
        assert!(state.interrupt.is_none() && state.suspended == 0);
        state.interrupt = Some(Box::new(interrupt));
        if state.outcome.is_some() {
            state.interrupt.as_ref().unwrap()();
        }
        Some(InterruptGuard(self.clone()))
    }

    /// Defer engine interruption while V8 callbacks convert values or borrow
    /// Guest Memory. Clear any already-issued interrupt under the same lock.
    #[cfg(feature = "v8")]
    pub(crate) fn suspend_interrupt(&self, clear: impl FnOnce()) -> InterruptSuspension {
        let mut state = self.0.lock().unwrap();
        state.suspended += 1;
        clear();
        InterruptSuspension(self.clone())
    }
}

pub(crate) struct HostWaitGuard(TerminationRequest);

impl Drop for HostWaitGuard {
    fn drop(&mut self) {
        self.0.0.lock().unwrap().host_wake = None;
    }
}

#[cfg(feature = "v8")]
pub(crate) struct InterruptGuard(TerminationRequest);

#[cfg(feature = "v8")]
impl Drop for InterruptGuard {
    fn drop(&mut self) {
        self.0.0.lock().unwrap().interrupt = None;
    }
}

#[cfg(feature = "v8")]
pub(crate) struct InterruptSuspension(TerminationRequest);

#[cfg(feature = "v8")]
impl Drop for InterruptSuspension {
    fn drop(&mut self) {
        let mut state = self.0.0.lock().unwrap();
        state.suspended -= 1;
        if state.suspended == 0
            && state.outcome.is_some()
            && let Some(interrupt) = &state.interrupt
        {
            interrupt();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receiver_drop_disconnects_all_terminators() {
        let (terminator, receiver) = termination_channel();
        let second = terminator.clone();
        drop(receiver);
        assert!(!terminator.terminate(RunTermination::Exit(1)));
        assert!(!second.terminate(RunTermination::Exit(2)));
    }

    #[test]
    fn guest_and_external_termination_share_the_first_outcome() {
        for guest_first in [true, false] {
            let (terminator, receiver) = termination_channel();
            if guest_first {
                receiver.request.request(RunTermination::Exit(7));
            }
            assert_eq!(
                terminator.terminate(RunTermination::KilledBySignal(9)),
                !guest_first
            );
            receiver.request.request(RunTermination::Exit(8));
            let expected = if guest_first {
                RunTermination::Exit(7)
            } else {
                RunTermination::KilledBySignal(9)
            };
            assert_eq!(receiver.request.get(), Some(expected));
            assert_eq!(receiver.close(), Some(expected));
            assert!(!terminator.terminate(RunTermination::Exit(0)));
        }
    }

    #[cfg(feature = "v8")]
    #[test]
    fn early_requests_interrupt_when_bound_and_unbinding_retires_the_target() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let (terminator, receiver) = termination_channel();
        assert!(terminator.terminate(RunTermination::Exit(7)));
        let calls = Arc::new(AtomicUsize::new(0));
        for _ in 0..2 {
            let calls = Arc::clone(&calls);
            let interrupt = receiver.request.bind_interrupt(move || {
                calls.fetch_add(1, Ordering::SeqCst);
            });
            drop(interrupt);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(!terminator.terminate(RunTermination::Exit(8)));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[cfg(feature = "v8")]
    #[test]
    fn host_calls_defer_interruption_until_the_outermost_call_returns() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let (terminator, receiver) = termination_channel();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&calls);
        let _interrupt = receiver.request.bind_interrupt(move || {
            observed.fetch_add(1, Ordering::SeqCst);
        });
        let outer = receiver.request.suspend_interrupt(|| {});
        let inner = receiver.request.suspend_interrupt(|| {});
        let wakes = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&wakes);
        let wait = receiver
            .request
            .bind_host_wake(|| {
                Ok::<_, ()>(move || {
                    observed.fetch_add(1, Ordering::SeqCst);
                })
            })
            .unwrap();
        let sender = std::thread::spawn(move || terminator.terminate(RunTermination::Exit(7)));
        assert!(sender.join().unwrap());
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        drop(wait);
        drop(inner);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        drop(outer);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn host_waits_retain_early_requests_and_retire_old_targets() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let (terminator, receiver) = termination_channel();
        let old = receiver
            .request
            .bind_host_wake(|| Ok::<_, ()>(|| panic!("retired wait woke")))
            .unwrap();
        drop(old);
        assert!(terminator.terminate(RunTermination::Exit(7)));

        let wakes = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&wakes);
        let wait = receiver
            .request
            .bind_host_wake(|| {
                Ok::<_, ()>(move || {
                    observed.fetch_add(1, Ordering::SeqCst);
                })
            })
            .unwrap();
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        drop(wait);
        receiver.close();
        assert!(
            receiver
                .request
                .bind_host_wake::<fn(), ()>(|| panic!("closed Run created a wake source"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn host_wakeup_stays_pending_until_poll_wait() {
        use crate::async_sys::internal::event_loop::poll;

        for terminate_before_binding in [true, false] {
            let (terminator, receiver) = termination_channel();
            let mut poll = poll::poll_create().unwrap();
            if terminate_before_binding {
                assert!(terminator.terminate(RunTermination::Exit(7)));
            }
            let _wait = receiver
                .request
                .bind_host_wake(|| {
                    let waker = poll::PollWaker::new(&poll)?;
                    Ok::<_, crate::async_host::AsyncHostError>(move || waker.wake().unwrap())
                })
                .unwrap();
            if !terminate_before_binding {
                assert!(terminator.terminate(RunTermination::Exit(7)));
            }
            // A zero timeout proves readiness was retained before the syscall,
            // including a request that arrived before registration itself.
            assert_eq!(poll::poll_wait(&mut poll, 0), Ok(1));
        }
    }

    #[test]
    fn records_killed_by_signal() {
        let request = TerminationRequest::default();

        request.request(RunTermination::KilledBySignal(15));

        assert_eq!(request.get(), Some(RunTermination::KilledBySignal(15)));
        assert_eq!(request.get(), Some(RunTermination::KilledBySignal(15)));
    }

    #[test]
    fn preserves_the_first_outcome() {
        let request = TerminationRequest::default();

        request.request(RunTermination::KilledBySignal(15));
        request.request(RunTermination::Exit(1));

        assert_eq!(request.get(), Some(RunTermination::KilledBySignal(15)));
    }
}
