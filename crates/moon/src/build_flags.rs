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

//! Translate shared CLI build arguments into executor configuration.

use moonbuild::execution::{BuildConfig, OutputStyle};
pub use moonutil::cli_support::BuildFlags;
use moonutil::features::FeatureGate;

pub(crate) fn execution_config(
    flags: &BuildFlags,
    unstable_features: &FeatureGate,
    verbose: bool,
) -> BuildConfig {
    BuildConfig {
        parallelism: flags.jobs,
        output_style: output_style(flags),
        render_no_loc: flags.render_no_loc,
        diagnostic_limit: flags.diagnostic_limit,
        n2_explain: unstable_features.rr_n2_explain,
        verbose,
        ..Default::default()
    }
}

pub(crate) fn output_style(flags: &BuildFlags) -> OutputStyle {
    match (flags.no_render, flags.output_json) {
        (true, false) => OutputStyle::Raw,
        (false, true) => OutputStyle::Json,
        (false, false) => OutputStyle::Fancy,
        (true, true) => unreachable!(
            "unreachable: both no_render and output_json flags are set (should be prevented by conflicts_with)"
        ),
    }
}
