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

//! Handles test promotion

use anyhow::Context;
use moonbuild::expect::{apply_expect, apply_snapshot};
use moonbuild_rupes_recta::discover::DiscoverResult;
use tracing::info;

use crate::run::PackageFilter;
use moonutil::build_options::TestIndexRange;

use super::{ReplaceableTestResults, TestCaseResult, TestResultKind};

/// Perform promotion on all test snapshots and expect tests met. Returns
/// the total number of tests promoted, along with a filter indicating which
/// tests needs rerunning.
pub(crate) fn perform_promotion(
    pkg_src: &DiscoverResult,
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
                        res.add_one(
                            *target,
                            Some(file),
                            Some(TestIndexRange::from_single(*idx)?),
                        );
                        to_update_snapshot.push(result);
                        count += 1;
                    }
                    TestResultKind::ExpectTestFailed => {
                        info!(?target, file, idx, "Need to update expect");
                        res.add_one(
                            *target,
                            Some(file),
                            Some(TestIndexRange::from_single(*idx)?),
                        );
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
    promote_all_snapshots(pkg_src, to_update_snapshot).context("Failed to promote snapshots")?;
    promote_all_expects(pkg_src, to_update_expect).context("Failed to promote expects")?;

    Ok((count, res))
}

/// Perform promotion on all test snapshots met.
fn promote_all_snapshots<'a>(
    pkg_src: &DiscoverResult,
    results: impl IntoIterator<Item = &'a TestCaseResult>,
) -> anyhow::Result<()> {
    apply_snapshot(pkg_src, results.into_iter().map(|x| x.raw.as_ref()))
}

/// Perform promotion on all expect tests met. Should fil
fn promote_all_expects<'a>(
    pkg_src: &DiscoverResult,
    results: impl IntoIterator<Item = &'a TestCaseResult>,
) -> anyhow::Result<()> {
    apply_expect(pkg_src, results.into_iter().map(|x| x.raw.as_ref()))
}
