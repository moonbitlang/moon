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
use tracing::info;

use crate::run::runtest::{
    ReplaceableTestResults, TestCaseResult, TestResultKind, filter::PackageFilter,
};

/// Perform promotion on all test snapshots and expect tests met. Returns
/// the total number of tests promoted, along with a filter indicating which
/// tests needs rerunning.
pub fn perform_promotion(
    results: &ReplaceableTestResults,
) -> anyhow::Result<(usize, PackageFilter)> {
    let mut res = PackageFilter::default();

    let mut to_update_snapshot = vec![];
    let mut to_update_expect = vec![];
    let mut count = 0;
    for (target, target_result) in &results.map {
        for (file, v) in &target_result.map {
            for (idx, result) in v {
                match result.kind {
                    TestResultKind::SnapshotTestFailed => {
                        info!(?target, file, idx, "Need to update snapshot");
                        res.add_one(*target, Some(file), Some(*idx));
                        to_update_snapshot.push(result);
                        count += 1;
                    }
                    TestResultKind::ExpectTestFailed => {
                        info!(?target, file, idx, "Need to update expect");
                        res.add_one(*target, Some(file), Some(*idx));
                        to_update_expect.push(result);
                        count += 1;
                    }
                    _ => {}
                }
            }
        }
    }

    // This is to be changed -- the original test promotion is too messy to work with.
    // We iterate through all results, filter those which are actually failed
    // snapshot tests, and then feed them to the `apply_snapshot` function.
    //
    // We are expecting these updates to work on batches, but the legacy call
    // site only supports single-file updates (i.e. only passed std::iter::once
    // to the functions).
    //
    // We will be very sad if it doesn't work.
    promote_all_snapshots(to_update_snapshot).context("Failed to promote snapshots")?;
    promote_all_expects(to_update_expect).context("Failed to promote expects")?;

    Ok((count, res))
}

/// Perform promotion on all test snapshots met.
fn promote_all_snapshots<'a>(
    results: impl IntoIterator<Item = &'a TestCaseResult>,
) -> anyhow::Result<()> {
    apply_snapshot(results.into_iter().map(|x| x.raw.message.as_str()))
}

/// Perform promotion on all expect tests met. Should fil
fn promote_all_expects<'a>(
    results: impl IntoIterator<Item = &'a TestCaseResult>,
) -> anyhow::Result<()> {
    apply_expect(results.into_iter().map(|x| x.raw.message.as_str()))
}
