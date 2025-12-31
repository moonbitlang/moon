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
pub use runtime::{CommandGuard, command_for};

use std::sync::OnceLock;

use tokio_util::sync::CancellationToken;
#[cfg(unix)]
use tracing::debug;

#[cfg(unix)]
static SHUTDOWN_TOKEN: OnceLock<CancellationToken> = OnceLock::new();
static SHUTDOWN_HANDLER: OnceLock<()> = OnceLock::new();

pub fn setup_shutdown_handler() {
    if SHUTDOWN_HANDLER.get().is_some() {
        return;
    }

    #[cfg(unix)]
    {
        let token = SHUTDOWN_TOKEN.get_or_init(CancellationToken::new).clone();
        use signal_hook::consts::signal::*;
        use signal_hook::iterator::Signals;

        let mut signals =
            Signals::new([SIGTERM, SIGINT, SIGQUIT]).expect("Failed to register signal handler");
        std::thread::spawn(move || {
            for signal in signals.forever() {
                debug!("Received termination signal: {:?}", signal);
                token.cancel();
            }
        });
    }

    let _ = SHUTDOWN_HANDLER.set(());
}

pub fn shutdown_token() -> Option<&'static CancellationToken> {
    #[cfg(windows)]
    {
        None
    }
    #[cfg(unix)]
    {
    SHUTDOWN_TOKEN.get()
    }
}

pub fn shutdown_requested() -> bool {
    shutdown_token().is_some_and(|token| token.is_cancelled())
}

pub fn default_rt() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}
