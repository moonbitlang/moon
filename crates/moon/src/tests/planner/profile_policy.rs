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

use moonutil::{build_options::RunMode, cond_expr::OptLevel};

use crate::build_flags::BuildFlags;

// Phase 1: profile selection is a pure flag-to-profile decision and should be
// tested without involving any planner graph.

#[test]
fn release_by_default_modes_match_flags() {
    for run_mode in [RunMode::Bench, RunMode::Bundle] {
        assert_eq!(
            BuildFlags::default().effective_profile(run_mode),
            OptLevel::Release
        );
    }
}

#[test]
fn debug_by_default_modes_match_flags() {
    for run_mode in [RunMode::Build, RunMode::Run, RunMode::Test, RunMode::Check] {
        assert_eq!(
            BuildFlags::default().effective_profile(run_mode),
            OptLevel::Debug
        );
    }
}

#[test]
fn explicit_release_overrides_every_mode() {
    let flags = BuildFlags {
        release: true,
        ..Default::default()
    };

    for run_mode in [
        RunMode::Build,
        RunMode::Run,
        RunMode::Test,
        RunMode::Check,
        RunMode::Bench,
        RunMode::Bundle,
    ] {
        assert_eq!(flags.effective_profile(run_mode), OptLevel::Release);
    }
}

#[test]
fn explicit_debug_overrides_every_mode() {
    let flags = BuildFlags {
        debug: true,
        ..Default::default()
    };

    for run_mode in [
        RunMode::Build,
        RunMode::Run,
        RunMode::Test,
        RunMode::Check,
        RunMode::Bench,
        RunMode::Bundle,
    ] {
        assert_eq!(flags.effective_profile(run_mode), OptLevel::Debug);
    }
}
