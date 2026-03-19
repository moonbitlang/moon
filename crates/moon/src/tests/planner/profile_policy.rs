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
fn bench_profile_selection_matches_flags() {
    assert_eq!(
        BuildFlags::default().effective_profile(RunMode::Bench),
        OptLevel::Release
    );
    assert_eq!(
        BuildFlags {
            release: true,
            ..Default::default()
        }
        .effective_profile(RunMode::Bench),
        OptLevel::Release
    );
    assert_eq!(
        BuildFlags {
            debug: true,
            ..Default::default()
        }
        .effective_profile(RunMode::Bench),
        OptLevel::Debug
    );
}

#[test]
fn test_profile_selection_matches_flags() {
    assert_eq!(
        BuildFlags::default().effective_profile(RunMode::Test),
        OptLevel::Debug
    );
    assert_eq!(
        BuildFlags {
            release: true,
            ..Default::default()
        }
        .effective_profile(RunMode::Test),
        OptLevel::Release
    );
    assert_eq!(
        BuildFlags {
            debug: true,
            ..Default::default()
        }
        .effective_profile(RunMode::Test),
        OptLevel::Debug
    );
}
