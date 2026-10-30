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

//! The virtual CLI's signal and exit policy, not a second execution host.

use std::sync::{Arc, Mutex};

use moonrun::{Engine, RunOutcome, RunTermination, SignalSender};

#[derive(Clone)]
pub(crate) struct Session {
    root_signals: SignalSender,
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    engine: Option<Engine>,
    shutdown: Option<RunTermination>,
    finished: bool,
}

impl Session {
    pub(crate) fn new(root_signals: SignalSender) -> Self {
        Self {
            root_signals,
            state: Default::default(),
        }
    }

    /// The broker must be installed before Engine initializes backend threads.
    /// Attachment and signal handling share a lock so an early signal closes
    /// admission before the caller can submit its root execution.
    pub(crate) fn attach_engine(&self, engine: &Engine) {
        let mut state = self.state.lock().unwrap();
        assert!(state.engine.is_none() && !state.finished);
        if let Some(outcome) = state.shutdown {
            engine.request_shutdown(outcome);
        }
        state.engine = Some(engine.clone());
    }

    pub(crate) fn send(&self, signal: i32) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.finished {
            return false;
        }
        // Once shutdown starts, repeated signals cannot skip cleanup by
        // falling back to host termination, or replace the first exit reason.
        if state.shutdown.is_some() || self.root_signals.send(signal) == Ok(true) {
            return true;
        }
        let outcome = RunTermination::KilledBySignal(signal);
        state.shutdown = Some(outcome);
        if let Some(engine) = &state.engine {
            engine.request_shutdown(outcome);
        }
        true
    }

    /// Called only after draining the Engine's adopted roots. An unhandled
    /// signal controls the CLI exit even when the original root had already
    /// returned; it does not rewrite that root's completed process result.
    /// Closing and reading the exit reason share the delivery lock: a signal
    /// either contributes to this result or returns to the OS adapter's
    /// fallback, never to an execution scope that has already finished.
    pub(crate) fn finish<E>(&self, root: Result<RunOutcome, E>) -> Result<RunOutcome, E> {
        let mut state = self.state.lock().unwrap();
        state.finished = true;
        state.engine = None;
        match state.shutdown {
            Some(outcome) => Ok(outcome.into()),
            None => root,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_and_repeated_signals_keep_the_first_outcome() {
        let (sender, _receiver) = moonrun::signal_channel();
        let session = Session::new(sender);
        assert!(session.send(2));
        assert!(session.send(15));

        let engine = Engine::default();
        session.attach_engine(&engine);
        assert!(!engine.request_shutdown(RunTermination::Exit(42)));
        assert!(session.send(1));
        // An admission error caused by the early signal must also present the
        // signal, rather than escape the CLI before its shutdown policy runs.
        assert_eq!(
            session.finish(Err("admission closed")),
            Ok(RunOutcome::KilledBySignal(2))
        );
        assert!(Engine::default().request_shutdown(RunTermination::Exit(0)));
    }

    #[test]
    fn a_signal_after_root_exit_still_shuts_down_the_session() {
        let (sender, receiver) = moonrun::signal_channel();
        let session = Session::new(sender);
        let engine = Engine::default();
        session.attach_engine(&engine);
        drop(receiver);
        assert!(session.send(15));
        assert!(!engine.request_shutdown(RunTermination::Exit(0)));
        assert_eq!(
            session.finish::<()>(Ok(RunOutcome::Completed)),
            Ok(RunOutcome::KilledBySignal(15))
        );
    }

    #[test]
    fn ordinary_root_completion_and_failure_do_not_request_shutdown() {
        for outcome in [
            Ok(RunOutcome::Completed),
            Ok(RunOutcome::Exited(7)),
            Err("root failed"),
        ] {
            let (sender, _receiver) = moonrun::signal_channel();
            let session = Session::new(sender);
            let engine = Engine::default();
            session.attach_engine(&engine);
            assert_eq!(session.finish(outcome), outcome);
            assert!(engine.request_shutdown(RunTermination::Exit(0)));
        }
    }

    #[test]
    fn attachment_and_signal_delivery_cannot_lose_shutdown() {
        let (sender, _receiver) = moonrun::signal_channel();
        let session = Session::new(sender);
        let engine = Engine::default();
        let gate = Arc::new(std::sync::Barrier::new(2));
        std::thread::scope(|threads| {
            let worker_gate = Arc::clone(&gate);
            let session = &session;
            threads.spawn(move || {
                worker_gate.wait();
                assert!(session.send(2));
            });
            gate.wait();
            session.attach_engine(&engine);
        });
        assert!(!engine.request_shutdown(RunTermination::Exit(0)));
        assert_eq!(
            session.finish::<()>(Ok(RunOutcome::Completed)),
            Ok(RunOutcome::KilledBySignal(2))
        );
    }

    #[test]
    fn finished_sessions_return_signals_without_touching_the_old_engine() {
        for cancel_before_finish in [false, true] {
            let (sender, _receiver) = moonrun::signal_channel();
            let session = Session::new(sender);
            let engine = Engine::default();
            session.attach_engine(&engine);
            if cancel_before_finish {
                assert!(session.send(2));
                assert!(session.send(15));
            }
            let expected = if cancel_before_finish {
                RunOutcome::KilledBySignal(2)
            } else {
                RunOutcome::Exited(7)
            };
            assert_eq!(
                session.finish::<()>(Ok(RunOutcome::Exited(7))),
                Ok(expected)
            );
            assert!(!session.send(1));
            assert_eq!(
                engine.request_shutdown(RunTermination::Exit(42)),
                !cancel_before_finish
            );
        }
    }

    #[test]
    fn a_signal_racing_finish_is_presented_or_returned_to_the_os_adapter() {
        let (sender, _receiver) = moonrun::signal_channel();
        let session = Session::new(sender);
        let gate = std::sync::Barrier::new(2);
        let (outcome, accepted) = std::thread::scope(|threads| {
            let signal = threads.spawn(|| {
                gate.wait();
                session.send(2)
            });
            gate.wait();
            let outcome = session.finish::<()>(Ok(RunOutcome::Completed));
            (outcome, signal.join().unwrap())
        });
        assert_eq!(
            outcome,
            Ok(if accepted {
                RunOutcome::KilledBySignal(2)
            } else {
                RunOutcome::Completed
            })
        );
        assert!(!session.send(15));
    }
}
