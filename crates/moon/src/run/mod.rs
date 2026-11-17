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
mod section_capture;

#[cfg(windows)]
pub(crate) use child::assign_process_to_job;
pub(crate) use child::run;
pub(crate) use runtest::{
    PackageFilter, ReplaceableTestResults, TestFilter, TestIndex, collect_test_invocations,
    collect_test_outline, perform_promotion, run_tests,
};
pub(crate) use runtime::{
    ExecutionMode, command_for, command_for_with_moonrun_policy,
    command_for_with_moonrun_policy_source_dir, moonrun_command,
};

use std::sync::{Arc, OnceLock};
use tokio::{sync::Semaphore, task::JoinSet};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

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

/// Build the command's Tokio runtime and install the global shutdown
/// listener. The caller must retain it across every backend and update pass,
/// driving it whenever children are running.
pub(crate) fn default_rt() -> std::io::Result<tokio::runtime::Runtime> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    install_shutdown_handler(&runtime);
    Ok(runtime)
}

/// Await bounded concurrent work, then restore invocation order. Concurrent runs
/// finish and are reaped before errors propagate; serial runs stop on the first error.
pub(crate) async fn run_with_parallelism<R: Send + 'static, E: Send + 'static>(
    runs: impl ExactSizeIterator<Item = impl Future<Output = Result<R, E>> + Send + 'static>,
    parallelism: usize,
) -> Result<Vec<R>, E> {
    let parallelism = parallelism.max(1).min(runs.len().max(1));
    if parallelism <= 1 {
        let mut results = Vec::new();
        for run in runs {
            results.push(run.await?);
        }
        return Ok(results);
    }

    let slots = Arc::new(Semaphore::new(parallelism));
    let mut tasks = JoinSet::new();
    for (index, run) in runs.enumerate() {
        let permit = Arc::clone(&slots)
            .acquire_owned()
            .await
            .expect("execution semaphore is never closed");
        tasks.spawn(
            async move {
                // Keep the slot until execution and result collection finish.
                let _permit = permit;
                (index, run.await)
            }
            .in_current_span(),
        );
    }

    let mut completed = Vec::new();
    while let Some(result) = tasks.join_next().await {
        completed.push(result);
    }
    // Drain all tasks before propagating either an execution error or a task panic.
    let mut results = completed
        .into_iter()
        .map(|result| result.expect("execution task panicked"))
        .collect::<Vec<_>>();
    results.sort_unstable_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}
