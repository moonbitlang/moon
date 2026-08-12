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

use crate::instance_signal::SignalReceiver;
use crate::{async_policy, v8_backend};
use anyhow::Context;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Process-wide runtime configuration for the current V8-backed implementation.
///
/// V8 flags are process-global and must be selected before the first run. All
/// [`Runtime`] values in one process therefore need the same configuration.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub(crate) stack_size: Option<String>,
}

impl RuntimeConfig {
    pub fn with_stack_size(mut self, stack_size: impl Into<String>) -> Self {
        self.stack_size = Some(stack_size.into());
        self
    }
}

/// Configuration for one MoonBit Wasm run.
#[derive(Debug, Default)]
pub struct RunOptions {
    pub(crate) args: Vec<String>,
    pub(crate) no_stack_trace: bool,
    pub(crate) test_args: Option<String>,
    pub(crate) policy_file: Option<PathBuf>,
    pub(crate) signal_receiver: Option<SignalReceiver>,
}

impl RunOptions {
    pub fn with_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn without_stack_trace(mut self) -> Self {
        self.no_stack_trace = true;
        self
    }

    pub fn with_test_args(mut self, test_args: impl Into<String>) -> Self {
        self.test_args = Some(test_args.into());
        self
    }

    pub fn with_policy_file(mut self, policy_file: impl Into<PathBuf>) -> Self {
        self.policy_file = Some(policy_file.into());
        self
    }

    /// Receives signals selected by the embedding process for this run.
    ///
    /// Moonrun never installs process signal handlers when used as a library.
    /// The embedding process owns that policy and may forward a signal through
    /// the paired [`crate::SignalSender`].
    pub fn with_signal_receiver(mut self, signal_receiver: SignalReceiver) -> Self {
        self.signal_receiver = Some(signal_receiver);
        self
    }
}

/// The observable result of one MoonBit Wasm run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunOutcome {
    Completed,
    Exited(i32),
    KilledBySignal(i32),
}

/// Embeddable Moonrun execution module.
///
/// Each call to [`Runtime::run_file`] creates fresh per-run host and Wasm
/// state. The current implementation still uses process stdio, environment,
/// working directory; those remain process-scoped dependencies to extract in
/// later changes. OS signal ownership stays outside this module and signals
/// may be injected through [`crate::signal_channel`]. Concurrent run semantics
/// are not yet part of this experimental interface.
#[derive(Clone, Debug)]
pub struct Runtime {
    config: RuntimeConfig,
}

impl Runtime {
    pub fn new(config: RuntimeConfig) -> Self {
        Self { config }
    }

    pub fn run_file(
        &self,
        file: impl AsRef<Path>,
        options: RunOptions,
    ) -> anyhow::Result<RunOutcome> {
        let async_policy = Arc::new(match options.policy_file.as_ref() {
            Some(path) => async_policy::AsyncPolicy::from_file(path).context(
                "failed to load sandbox policy (experimental); run `moonrun --help` for policy format notes",
            )?,
            None => async_policy::AsyncPolicy::allow_all(),
        });

        let file = file.as_ref();
        if !file.exists() {
            anyhow::bail!("no such file");
        }
        if file.extension().and_then(|extension| extension.to_str()) != Some("wasm") {
            anyhow::bail!("Unsupported file type");
        }

        v8_backend::run(&self.config, file, options, async_policy)
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new(RuntimeConfig::default())
    }
}
