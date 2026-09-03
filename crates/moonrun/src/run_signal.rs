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

use std::fmt;
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[cfg(windows)]
use crate::async_sys::internal::event_loop::poll::{self, CompletionPort};
#[cfg(unix)]
use crate::async_sys::signal::SigwaitTarget;

/// The sending half of one Run's signal channel.
///
/// Sending does not raise an operating-system signal. The caller chooses
/// which Run receives the signal by choosing its sender.
#[derive(Clone)]
pub struct SignalSender {
    shared: Arc<SignalState>,
}

/// The receiving half of one Run's signal channel.
///
/// Pass this value to [`crate::Engine::run_with_signal_receiver`]. A receiver
/// belongs to exactly one Run and is intentionally not cloneable.
pub struct SignalReceiver {
    shared: Arc<SignalState>,
}

#[cfg(unix)]
pub(crate) struct SigwaitTargetGuard {
    shared: Arc<SignalState>,
}

/// An error returned when a signal cannot be delivered to a Run.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum SignalSendError {
    #[error("invalid Run signal")]
    InvalidSignal,
    #[error("the Run signal receiver is disconnected")]
    Disconnected,
    #[error("failed to wake the Run event loop")]
    DeliveryFailed,
}

/// Create a channel for directing signals to one Run.
pub fn signal_channel() -> (SignalSender, SignalReceiver) {
    let shared = Arc::new(SignalState {
        inner: Mutex::new(SignalStateInner {
            receiver_alive: true,
            interested: 0,
            #[cfg(unix)]
            target: None,
            #[cfg(windows)]
            target: None,
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
    /// Deliver a signal to this channel's Run.
    ///
    /// `Ok(true)` means the guest accepted the signal. `Ok(false)` means the
    /// guest has not registered interest in it, so a process adapter may apply
    /// its platform's default behavior instead.
    pub fn send(&self, signal: i32) -> Result<bool, SignalSendError> {
        if signal < 0 {
            return Err(SignalSendError::InvalidSignal);
        }
        let bit = signal_bit(signal);

        #[cfg(unix)]
        {
            let target = {
                let state = self
                    .shared
                    .inner
                    .lock()
                    .map_err(|_| SignalSendError::Disconnected)?;
                if !state.receiver_alive {
                    return Err(SignalSendError::Disconnected);
                }
                if state.interested & bit == 0 {
                    return Ok(false);
                }
                state.target.clone()
            };
            target.map_or(Ok(false), |target| {
                target
                    .send(bit)
                    .map_err(|_| SignalSendError::DeliveryFailed)
            })
        }

        #[cfg(windows)]
        {
            let event = encode_signal_event(signal);
            let target = {
                let state = self
                    .shared
                    .inner
                    .lock()
                    .map_err(|_| SignalSendError::Disconnected)?;
                if !state.receiver_alive {
                    return Err(SignalSendError::Disconnected);
                }
                if state.interested & bit == 0 {
                    return Ok(false);
                }
                let Some(target) = state.target.clone() else {
                    return Ok(false);
                };
                target
            };
            poll::post_thread_pool_completion(&target, event)
                .map_err(|_| SignalSendError::DeliveryFailed)?;
            Ok(true)
        }
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
    pub(crate) fn configure(&self, all_signals: &[i32], signals: &[i32]) {
        self.shared.inner.lock().unwrap().interested =
            signal_mask(signals) & signal_mask(all_signals);
    }

    #[cfg(unix)]
    pub(crate) fn attach_target(&self, target: SigwaitTarget) -> Option<SigwaitTargetGuard> {
        let Ok(mut state) = self.shared.inner.lock() else {
            return None;
        };
        if !state.receiver_alive || state.target.is_some() {
            return None;
        }
        state.target = Some(target);
        Some(SigwaitTargetGuard {
            shared: Arc::clone(&self.shared),
        })
    }

    #[cfg(windows)]
    pub(crate) fn attach_target(&self, target: CompletionPort) {
        self.shared.inner.lock().unwrap().target = Some(target);
    }

    #[cfg(windows)]
    pub(crate) fn detach_target(&self) {
        self.shared.inner.lock().unwrap().target = None;
    }
}

#[cfg(unix)]
impl Drop for SigwaitTargetGuard {
    fn drop(&mut self) {
        self.shared.inner.lock().unwrap().target = None;
    }
}

impl Drop for SignalReceiver {
    fn drop(&mut self) {
        let mut state = self.shared.inner.lock().unwrap();
        state.receiver_alive = false;
        state.target = None;
    }
}

struct SignalState {
    inner: Mutex<SignalStateInner>,
}

struct SignalStateInner {
    receiver_alive: bool,
    interested: u32,
    #[cfg(unix)]
    target: Option<SigwaitTarget>,
    #[cfg(windows)]
    target: Option<CompletionPort>,
}

#[cfg(windows)]
fn encode_signal_event(signal: i32) -> i32 {
    ((signal as u32) | (1_u32 << 31)) as i32
}

fn signal_bit(signal: i32) -> u32 {
    u32::try_from(signal)
        .ok()
        .and_then(|signal| 1_u32.checked_shl(signal))
        .unwrap_or(0)
}

pub(crate) fn signal_mask(signals: &[i32]) -> u32 {
    signals
        .iter()
        .copied()
        .fold(0, |mask, signal| mask | signal_bit(signal))
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

    #[cfg(windows)]
    #[test]
    fn windows_signals_are_delivered_only_to_the_selected_run() {
        use windows_sys::Win32::System::Console::CTRL_BREAK_EVENT;

        let signal = CTRL_BREAK_EVENT as i32;
        let (first_sender, first_receiver) = signal_channel();
        let (second_sender, second_receiver) = signal_channel();
        first_receiver.configure(&[signal], &[signal]);
        second_receiver.configure(&[signal], &[signal]);

        let mut first_poll = poll::poll_create().unwrap();
        let mut second_poll = poll::poll_create().unwrap();
        first_receiver.attach_target(CompletionPort::from_poll(&first_poll));
        second_receiver.attach_target(CompletionPort::from_poll(&second_poll));

        assert_eq!(first_sender.send(signal), Ok(true));
        assert_eq!(poll::poll_wait(&mut first_poll, 0).unwrap(), 1);
        assert_eq!(poll::poll_wait(&mut second_poll, 0).unwrap(), 0);
        let event = poll::event_list_get(&first_poll, 0).unwrap();
        assert_eq!(
            poll::event_get_bytes_transferred(event),
            encode_signal_event(signal) as u32
        );

        assert_eq!(second_sender.send(signal), Ok(true));
        assert_eq!(poll::poll_wait(&mut second_poll, 0).unwrap(), 1);
        assert_eq!(poll::poll_wait(&mut first_poll, 0).unwrap(), 0);
        let event = poll::event_list_get(&second_poll, 0).unwrap();
        assert_eq!(
            poll::event_get_bytes_transferred(event),
            encode_signal_event(signal) as u32
        );

        drop(first_receiver);
        drop(second_receiver);
        poll::poll_destroy(first_poll);
        poll::poll_destroy(second_poll);
    }
}
