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

use moonutil::{common::RunMode, cond_expr::OptLevel};

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
