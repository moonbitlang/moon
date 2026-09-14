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
/// Ensures we only install the shutdown handler task once per process.
static SHUTDOWN_HANDLER: OnceLock<()> = OnceLock::new();

fn install_shutdown_handler(rt: &tokio::runtime::Runtime) {
    SHUTDOWN_HANDLER.get_or_init(|| {
        let token = SHUTDOWN_TOKEN.get_or_init(CancellationToken::new).clone();
        let handle = rt.handle().clone();
        handle.spawn(async move {
            #[cfg(not(windows))]
            {
                let mut terminate =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("Failed to wait on SigTerm");
                let mut interrupt =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                        .expect("Failed to wait on SigInt");
                let mut quit = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::quit())
                    .expect("Failed to wait on SigQuit");
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
                let mut ctrl_c =
                    tokio::signal::windows::ctrl_c().expect("Failed to wait on ctrl+c");
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

/// Build the canonical Tokio runtime used by `moon run` facilities and install
/// the global shutdown handler the first time it is called.
pub(crate) fn default_rt() -> std::io::Result<tokio::runtime::Runtime> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    install_shutdown_handler(&runtime);
    Ok(runtime)
}
