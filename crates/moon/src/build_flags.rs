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

use anyhow::bail;
use moonutil::{
    common::{DiagnosticLevel, RunMode, SurfaceTarget, TargetBackend},
    cond_expr::OptLevel as BuildProfile,
};

#[derive(Debug, clap::Parser, Clone)]
pub struct BuildFlags {
    /// Enable the standard library (default)
    #[clap(long)]
    pub(crate) std: bool,

    /// Disable the standard library
    #[clap(long, long = "nostd")]
    pub(crate) no_std: bool,

    /// Emit debug information
    #[clap(long, short = 'g')]
    pub debug: bool,

    /// Compile in release mode
    #[clap(long, conflicts_with = "debug")]
    pub release: bool,

    /// Enable stripping debug information
    #[clap(long, conflicts_with = "no_strip")]
    pub strip: bool,

    /// Disable stripping debug information
    #[clap(long, conflicts_with = "strip")]
    pub no_strip: bool,

    /// Select output target
    #[clap(long, value_delimiter = ',')]
    pub target: Vec<SurfaceTarget>,

    /// [Deprecated] Handle the selected targets sequentially
    ///
    /// This flag is deprecated, because all targets are handled sequentially
    /// for now, until multi-target compilation is implemented (if any).
    #[clap(long, requires = "target", hide = true)]
    #[deprecated]
    pub serial: bool,

    /// Enable coverage instrumentation
    #[clap(long)]
    pub enable_coverage: bool,

    /// Sort input files
    #[clap(long)]
    pub sort_input: bool,

    /// Output WAT instead of WASM
    // TODO: we need a more general name, like `--emit-asm` or even `--emit={binary,asm}`
    #[clap(long)]
    pub output_wat: bool,

    /// Treat all warnings as errors
    #[clap(long, short)]
    pub deny_warn: bool,

    /// Don't render diagnostics (in raw human-readable format)
    #[clap(long)]
    pub no_render: bool,

    /// Output diagnostics in JSON format
    #[clap(long, conflicts_with = "no_render")]
    pub output_json: bool,

    /// Warn list config
    #[clap(long, allow_hyphen_values = true)]
    pub warn_list: Option<String>,

    /// Enable value tracing
    #[clap(long, hide = true)]
    pub enable_value_tracing: bool,

    /// Set the max number of jobs to run in parallel
    #[clap(short = 'j', long)]
    pub jobs: Option<usize>,

    /// Render no-location diagnostics starting from a certain level
    #[clap(long, value_name = "MIN_LEVEL", default_value = "error")]
    pub render_no_loc: DiagnosticLevel,
}

impl Default for BuildFlags {
    #[allow(deprecated)]
    fn default() -> Self {
        Self {
            std: false,
            no_std: false,
            debug: false,
            release: false,
            strip: false,
            no_strip: false,
            target: Vec::new(),
            serial: false,
            enable_coverage: false,
            sort_input: false,
            output_wat: false,
            deny_warn: false,
            no_render: false,
            output_json: false,
            warn_list: None,
            enable_value_tracing: false,
            jobs: None,
            render_no_loc: DiagnosticLevel::Error,
        }
    }
}

impl BuildFlags {
    pub fn resolve_single_target_backend(&self) -> anyhow::Result<Option<TargetBackend>> {
        if self.target.is_empty() {
            return Ok(None);
        }
        let targets = &self.target;

        if targets.len() > 1 {
            bail!("`--target` only supports one target backend");
        }
        let backends = moonutil::common::lower_surface_targets(targets);
        if backends.len() == 1 {
            Ok(Some(backends[0]))
        } else {
            bail!("`--target` only supports one target backend");
        }
    }
}

/// The style to render diagnostics in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputStyle {
    /// The human-readable raw format directly from `moonc`
    Raw,
    /// Source code snippets with colors and formatting, rendered from JSON
    Fancy,
    /// Machine-readable output in JSON
    Json,
}

impl OutputStyle {
    /// Whether the output style requires `moonc` to emit JSON diagnostics.
    pub fn needs_moonc_json(&self) -> bool {
        matches!(self, OutputStyle::Fancy | OutputStyle::Json)
    }

    /// Whether the output style requires no rendering (i.e., raw diagnostics).
    pub fn needs_no_render(&self) -> bool {
        matches!(self, OutputStyle::Raw | OutputStyle::Json)
    }
}

impl BuildFlags {
    pub fn std(&self) -> bool {
        match (self.std, self.no_std) {
            (false, false) => true,
            (true, false) => true,
            (false, true) => false,
            (true, true) => panic!("both std and no_std flags are set"),
        }
    }

    pub fn apply_default_debug(&mut self) {
        if !self.debug && !self.release {
            self.debug = true;
        }
    }

    pub fn strip(&self) -> bool {
        if self.strip {
            true
        } else if self.no_strip {
            false
        } else {
            !self.debug
        }
    }

    /// Resolve the effective build profile for Rupes Recta compilation.
    pub fn effective_profile(&self, run_mode: RunMode) -> BuildProfile {
        if self.debug {
            BuildProfile::Debug
        } else if self.release {
            BuildProfile::Release
        } else {
            match run_mode {
                RunMode::Bench | RunMode::Bundle => BuildProfile::Release,
                RunMode::Build
                | RunMode::Run
                | RunMode::Test
                | RunMode::Check
                | RunMode::Prove
                | RunMode::Format => BuildProfile::Debug,
            }
        }
    }

    /// Resolve whether to strip debug info for Rupes Recta compilation.
    pub fn strip_for(&self, run_mode: RunMode) -> bool {
        if self.strip {
            true
        } else if self.no_strip {
            false
        } else {
            self.effective_profile(run_mode) == BuildProfile::Release
        }
    }

    /// Resolve whether to emit debug symbols for Rupes Recta compilation.
    pub fn debug_symbols_for(&self, run_mode: RunMode) -> bool {
        !self.strip_for(run_mode)
    }

    pub fn output_style(&self) -> OutputStyle {
        match (self.no_render, self.output_json) {
            (true, false) => OutputStyle::Raw,
            (false, true) => OutputStyle::Json,
            (false, false) => OutputStyle::Fancy,
            (true, true) => unreachable!(
                "unreachable: both no_render and output_json flags are set (should be prevented by conflicts_with)"
            ),
        }
    }
}
