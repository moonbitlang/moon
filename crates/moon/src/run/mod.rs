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

//! `moon run` facility and friends

mod child;
mod runtest;
mod runtime;

pub use child::run;
pub use runtest::{TestFilter, TestIndex, perform_promotion, run_tests};
pub use runtime::command_for;

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
pub fn shutdown_token() -> &'static CancellationToken {
    SHUTDOWN_TOKEN.get_or_init(CancellationToken::new)
}

/// Check whether shutdown has been requested via any of the registered signals.
pub fn shutdown_requested() -> bool {
    shutdown_token().is_cancelled()
}

/// Build the canonical Tokio runtime used by `moon run` facilities and install
/// the global shutdown handler the first time it is called.
pub fn default_rt() -> std::io::Result<tokio::runtime::Runtime> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    install_shutdown_handler(&runtime);
    Ok(runtime)
}
