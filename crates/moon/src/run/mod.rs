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

//! `moon run` facility and friends

mod child;
mod runtest;
mod runtime;

#[cfg(windows)]
pub(crate) use child::assign_process_to_job;
pub(crate) use child::run;
pub(crate) use runtest::{
    PackageFilter, ReplaceableTestResults, TestFilter, TestIndex, collect_test_invocations,
    collect_test_outline, perform_promotion, run_tests,
};
pub(crate) use runtime::{
    ExecutionMode, command_for, command_for_with_moonrun_policy,
    command_for_with_moonrun_policy_source_dir,
};

use std::sync::OnceLock;

use tokio_util::sync::CancellationToken;

/// Process-wide cancellation token toggled when we observe a shutdown signal.
static SHUTDOWN_TOKEN: OnceLock<CancellationToken> = OnceLock::new();
/// Tokio retains its signal registrations after cleanup, so a test runtime
/// cannot be restarted in this process after restoring the incoming handlers.
static TEST_RUNTIME_STARTED: OnceLock<()> = OnceLock::new();

#[cfg(unix)]
const SHUTDOWN_SIGNALS: [libc::c_int; 3] = [libc::SIGTERM, libc::SIGINT, libc::SIGQUIT];

fn install_shutdown_handler(rt: &tokio::runtime::Runtime) {
    let token = SHUTDOWN_TOKEN.get_or_init(CancellationToken::new).clone();
    rt.spawn(async move {
        #[cfg(unix)]
        {
            let [mut terminate, mut interrupt, mut quit] = SHUTDOWN_SIGNALS.map(|signal| {
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::from_raw(signal))
                    .expect("Failed to wait on shutdown signal")
            });
            tokio::select! {
                _ = terminate.recv() => {},
                _ = interrupt.recv() => {},
                _ = quit.recv() => {},
            }
            token.cancel();
        }
        #[cfg(windows)]
        {
            let mut ctrl_break =
                tokio::signal::windows::ctrl_break().expect("Failed to wait on ctrl+break");
            let mut ctrl_c = tokio::signal::windows::ctrl_c().expect("Failed to wait on ctrl+c");
            let mut ctrl_close =
                tokio::signal::windows::ctrl_close().expect("Failed to wait on ctrl+close");
            tokio::select! {
                _ = ctrl_break.recv() => {},
                _ = ctrl_c.recv() => {},
                _ = ctrl_close.recv() => {},
            }
            token.cancel();
        }
    });
}

/// Return the shared shutdown token, initializing it lazily on first use.
pub(crate) fn shutdown_token() -> &'static CancellationToken {
    SHUTDOWN_TOKEN.get_or_init(CancellationToken::new)
}

/// Check whether shutdown has been requested via any of the registered signals.
pub(crate) fn shutdown_requested() -> bool {
    shutdown_token().is_cancelled()
}

/// Owns the test runtime and its process-wide shutdown handlers. Retain it
/// across every backend and update pass. Dropping it ends the test phase and
/// restores incoming Unix signal dispositions, including inherited ignores.
pub(crate) struct TestRuntime {
    // Taken during Drop so the listener stops before its handlers are restored.
    runtime: Option<tokio::runtime::Runtime>,
    #[cfg(unix)]
    signal_handlers: [(libc::c_int, libc::sigaction); SHUTDOWN_SIGNALS.len()],
}

impl TestRuntime {
    pub(crate) fn new() -> std::io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        #[cfg(unix)]
        let mut signal_handlers = SHUTDOWN_SIGNALS.map(|signal| {
            // SAFETY: sigaction is plain C data, filled by the query below.
            (signal, unsafe { std::mem::zeroed::<libc::sigaction>() })
        });
        #[cfg(unix)]
        for (signal, handler) in &mut signal_handlers {
            // SAFETY: the signal is valid and handler is a writable sigaction.
            if unsafe { libc::sigaction(*signal, std::ptr::null(), handler) } != 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        TEST_RUNTIME_STARTED
            .set(())
            .map_err(|()| std::io::Error::other("test runtime can only be started once"))?;
        install_shutdown_handler(&runtime);
        Ok(Self {
            runtime: Some(runtime),
            #[cfg(unix)]
            signal_handlers,
        })
    }

    pub(crate) fn block_on<F: std::future::Future>(&self, future: F) -> F::Output {
        self.runtime
            .as_ref()
            .expect("test runtime is present until drop")
            .block_on(future)
    }
}

impl Drop for TestRuntime {
    fn drop(&mut self) {
        drop(self.runtime.take());
        #[cfg(unix)]
        for (signal, handler) in &self.signal_handlers {
            // SAFETY: restore the complete disposition returned by sigaction.
            if unsafe { libc::sigaction(*signal, handler, std::ptr::null_mut()) } != 0 {
                tracing::error!(
                    signal,
                    error = %std::io::Error::last_os_error(),
                    "Failed to restore test signal handler"
                );
            }
        }
    }
}
