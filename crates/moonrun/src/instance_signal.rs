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

use crate::run_termination::{RunTermination, TerminationRequest};
use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::sync::{Arc, Mutex};

#[cfg(unix)]
use crate::async_sys::internal::event_loop::ThreadPoolCompletionNotifier;
#[cfg(windows)]
use crate::async_sys::internal::event_loop::poll::{self, CompletionPort};

/// The sending half of one Wasm instance's signal channel.
///
/// This handle is thread safe. Sending does not raise an operating-system
/// signal or affect any other instance in the embedding process.
#[derive(Clone)]
pub struct SignalSender {
    shared: Arc<SignalState>,
}

/// The receiving half of one Wasm instance's signal channel.
///
/// Pass this value to [`crate::RunOptions::with_signal_receiver`]. A receiver
/// belongs to one run and is intentionally not cloneable.
pub struct SignalReceiver {
    shared: Arc<SignalState>,
}

/// An error returned when a signal cannot be delivered to an instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalSendError {
    InvalidSignal,
    Disconnected,
    DeliveryFailed,
}

impl fmt::Display for SignalSendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSignal => f.write_str("invalid instance signal"),
            Self::Disconnected => f.write_str("the instance signal receiver is disconnected"),
            Self::DeliveryFailed => f.write_str("failed to wake the instance event loop"),
        }
    }
}

impl std::error::Error for SignalSendError {}

/// Creates a channel for injecting signals into one Wasm run.
pub fn signal_channel() -> (SignalSender, SignalReceiver) {
    let shared = Arc::new(SignalState {
        inner: Mutex::new(SignalStateInner {
            receiver_alive: true,
            engine: None,
            interested: None,
            target: None,
            cooperative_delivery_active: false,
            pending: VecDeque::new(),
        }),
    });
    (
        SignalSender {
            shared: Arc::clone(&shared),
        },
        SignalReceiver { shared },
    )
}

impl SignalSender {
    /// Returns `true` when the instance accepted the signal and `false` when
    /// the guest explicitly excluded it from its cancellation signal set.
    pub fn send(&self, signal: i32) -> Result<bool, SignalSendError> {
        let event = encode_signal_event(signal)?;
        let action = {
            let mut state = self.shared.inner.lock().unwrap();
            if !state.receiver_alive {
                return Err(SignalSendError::Disconnected);
            }
            if state
                .interested
                .as_ref()
                .is_some_and(|signals| !signals.contains(&signal))
            {
                return Ok(false);
            }

            if state.cooperative_delivery_active
                && state.interested.is_some()
                && let Some(target) = state.target.clone()
            {
                SignalAction::Notify(target, event)
            } else if state.target.is_some() || state.interested.is_some() {
                // Async event-loop setup is in progress. Preserve ordering and
                // let activation flush signals after both policy and waker are
                // installed.
                state.pending.push_back(signal);
                SignalAction::None
            } else if let Some((engine, termination)) = state.engine.clone() {
                SignalAction::Terminate(engine, termination, signal)
            } else {
                // The sender may be used before Runtime::run_file starts.
                state.pending.push_back(signal);
                SignalAction::None
            }
        };
        action.perform()?;
        Ok(true)
    }
}

impl fmt::Debug for SignalSender {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignalSender").finish_non_exhaustive()
    }
}

impl fmt::Debug for SignalReceiver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignalReceiver").finish_non_exhaustive()
    }
}

impl SignalReceiver {
    pub(crate) fn attach_engine(
        &self,
        engine: v8::IsolateHandle,
        termination: TerminationRequest,
    ) -> EngineSignalAttachment {
        let action = {
            let mut state = self.shared.inner.lock().unwrap();
            state.engine = Some((engine.clone(), termination.clone()));
            if state.target.is_none() && state.interested.is_none() {
                state
                    .pending
                    .pop_front()
                    .map(|signal| SignalAction::Terminate(engine, termination, signal))
            } else {
                None
            }
        };
        if let Some(action) = action {
            let _ = action.perform();
        }
        EngineSignalAttachment {
            shared: Arc::clone(&self.shared),
        }
    }

    pub(crate) fn configure(
        &self,
        all_signals: &[i32],
        signals: &[i32],
    ) -> Result<(), SignalSendError> {
        let all_signals = all_signals.iter().copied().collect::<HashSet<_>>();
        let interested = signals
            .iter()
            .copied()
            .filter(|signal| all_signals.contains(signal) && *signal >= 0)
            .collect::<HashSet<_>>();
        let actions = {
            let mut state = self.shared.inner.lock().unwrap();
            state.pending.retain(|signal| interested.contains(signal));
            state.interested = Some(interested);
            state.ready_actions()
        };
        perform_all(actions)
    }

    #[cfg(unix)]
    pub(crate) fn attach_target(
        &self,
        notifier: Arc<ThreadPoolCompletionNotifier>,
    ) -> Result<(), SignalSendError> {
        let actions = {
            let mut state = self.shared.inner.lock().unwrap();
            state.target = Some(SignalTarget::Unix(notifier));
            state.ready_actions()
        };
        perform_all(actions)
    }

    #[cfg(windows)]
    pub(crate) fn attach_target(
        &self,
        completion_port: CompletionPort,
    ) -> Result<(), SignalSendError> {
        let actions = {
            let mut state = self.shared.inner.lock().unwrap();
            state.target = Some(SignalTarget::Windows(completion_port));
            state.ready_actions()
        };
        perform_all(actions)
    }

    pub(crate) fn detach_target(&self) {
        self.shared.inner.lock().unwrap().target = None;
    }

    pub(crate) fn activate(&self) -> Result<(), SignalSendError> {
        let actions = {
            let mut state = self.shared.inner.lock().unwrap();
            state.cooperative_delivery_active = true;
            state.ready_actions()
        };
        perform_all(actions)
    }

    pub(crate) fn deactivate(&self) {
        self.shared
            .inner
            .lock()
            .unwrap()
            .cooperative_delivery_active = false;
    }
}

impl Drop for SignalReceiver {
    fn drop(&mut self) {
        let mut state = self.shared.inner.lock().unwrap();
        state.receiver_alive = false;
        state.engine = None;
        state.target = None;
        state.pending.clear();
    }
}

pub(crate) struct EngineSignalAttachment {
    shared: Arc<SignalState>,
}

impl Drop for EngineSignalAttachment {
    fn drop(&mut self) {
        self.shared.inner.lock().unwrap().engine = None;
    }
}

struct SignalState {
    inner: Mutex<SignalStateInner>,
}

struct SignalStateInner {
    receiver_alive: bool,
    engine: Option<(v8::IsolateHandle, TerminationRequest)>,
    interested: Option<HashSet<i32>>,
    target: Option<SignalTarget>,
    cooperative_delivery_active: bool,
    pending: VecDeque<i32>,
}

impl SignalStateInner {
    fn ready_actions(&mut self) -> Vec<SignalAction> {
        if !self.cooperative_delivery_active {
            return Vec::new();
        }
        let (Some(target), Some(interested)) = (&self.target, &self.interested) else {
            return Vec::new();
        };
        let target = target.clone();
        self.pending
            .drain(..)
            .filter(|signal| interested.contains(signal))
            .filter_map(|signal| {
                encode_signal_event(signal)
                    .ok()
                    .map(|event| SignalAction::Notify(target.clone(), event))
            })
            .collect()
    }
}

#[derive(Clone)]
enum SignalTarget {
    #[cfg(unix)]
    Unix(Arc<ThreadPoolCompletionNotifier>),
    #[cfg(windows)]
    Windows(CompletionPort),
}

impl SignalTarget {
    fn notify(&self, event: i32) -> Result<(), SignalSendError> {
        match self {
            #[cfg(unix)]
            Self::Unix(notifier) => notifier
                .notify(event)
                .map_err(|_| SignalSendError::DeliveryFailed),
            #[cfg(windows)]
            Self::Windows(completion_port) => {
                poll::post_thread_pool_completion(completion_port, event)
                    .map_err(|_| SignalSendError::DeliveryFailed)
            }
        }
    }
}

enum SignalAction {
    None,
    Notify(SignalTarget, i32),
    Terminate(v8::IsolateHandle, TerminationRequest, i32),
}

impl SignalAction {
    fn perform(self) -> Result<(), SignalSendError> {
        match self {
            Self::None => Ok(()),
            Self::Notify(target, event) => target.notify(event),
            Self::Terminate(engine, termination, signal) => {
                termination.request(RunTermination::KilledBySignal(signal));
                if engine.terminate_execution() {
                    Ok(())
                } else {
                    Err(SignalSendError::Disconnected)
                }
            }
        }
    }
}

fn perform_all(actions: Vec<SignalAction>) -> Result<(), SignalSendError> {
    actions.into_iter().try_for_each(SignalAction::perform)
}

fn encode_signal_event(signal: i32) -> Result<i32, SignalSendError> {
    if signal < 0 {
        return Err(SignalSendError::InvalidSignal);
    }
    Ok(((signal as u32) | (1_u32 << 31)) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_negative_signals() {
        let (sender, _receiver) = signal_channel();
        assert_eq!(sender.send(-1), Err(SignalSendError::InvalidSignal));
    }

    #[test]
    fn sender_disconnects_with_receiver() {
        let (sender, receiver) = signal_channel();
        drop(receiver);
        assert_eq!(sender.send(2), Err(SignalSendError::Disconnected));
    }

    #[test]
    fn guest_signal_selection_filters_pending_signals() {
        let (sender, receiver) = signal_channel();
        sender.send(2).unwrap();
        receiver.configure(&[1, 2, 3], &[1, 3]).unwrap();

        let state = receiver.shared.inner.lock().unwrap();
        assert!(state.pending.is_empty());
        drop(state);
        assert_eq!(sender.send(2), Ok(false));
        assert!(receiver.shared.inner.lock().unwrap().pending.is_empty());
    }
}
