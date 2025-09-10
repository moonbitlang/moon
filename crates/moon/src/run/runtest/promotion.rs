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

//! Handles test promotion

use anyhow::Context;
use moonbuild::expect::{apply_expect, apply_snapshot};

use crate::run::runtest::{TestCaseResult, TestResultKind};

/// Perform promotion on all test snapshots met.
pub fn promote_all_snapshots(results: &[TestCaseResult]) -> anyhow::Result<()> {
    // This is to be changed -- the original test promotion is too messy to work with.
    // We iterate through all results, filter those which are actually failed
    // snapshot tests, and then feed them to the `apply_snapshot` function.
    //
    // If you're looking for the list of tests to be rerun after promotion,
    // it's calculated when after the promotions are done. (Simple filtering
    // can't do any harm right?)
    apply_snapshot(
        results
            .iter()
            .filter(|&x| (x.kind == TestResultKind::SnapshotTestFailed))
            .map(|x| x.raw.message.as_str()),
    )
    .context("Failed to apply snapshot updates")
}

/// Perform promotion on all expect tests met.
pub fn promote_all_expects(results: &[TestCaseResult]) -> anyhow::Result<()> {
    apply_expect(
        results
            .iter()
            .filter(|x| x.kind == TestResultKind::ExpectTestFailed)
            .map(|x| x.raw.message.as_str()),
    )
    .context("Failed to apply expect updates")
}
