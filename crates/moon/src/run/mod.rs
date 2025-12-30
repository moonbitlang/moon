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
use tracing::debug;

static SHUTDOWN_TOKEN: OnceLock<CancellationToken> = OnceLock::new();
static SHUTDOWN_HANDLER: OnceLock<()> = OnceLock::new();

pub fn setup_shutdown_handler() {
    if SHUTDOWN_HANDLER.get().is_some() {
        return;
    }
    let token = SHUTDOWN_TOKEN
        .get_or_init(CancellationToken::new)
        .clone();

    #[cfg(unix)]
    {
        use signal_hook::consts::signal::*;
        use signal_hook::iterator::Signals;

        let mut signals = Signals::new([SIGTERM, SIGINT, SIGQUIT])
            .expect("Failed to register signal handler");
        std::thread::spawn(move || {
            for signal in signals.forever() {
                debug!("Received termination signal: {:?}", signal);
                token.cancel();
            }
        });
    }

    #[cfg(windows)]
    {
        let token = token.clone();
        ctrlc::set_handler(move || {
            debug!("Received termination signal");
            token.cancel();
        })
        .expect("Failed to register Ctrl-C handler");
    }

    let _ = SHUTDOWN_HANDLER.set(());
}

pub fn shutdown_token() -> Option<&'static CancellationToken> {
    SHUTDOWN_TOKEN.get()
}

pub fn default_rt() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}
