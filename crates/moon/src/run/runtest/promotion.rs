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

use std::sync::Arc;

use anyhow::Context;
use moonbuild::expect::PackageSrcResolver;
use moonbuild::expect::{apply_expect, apply_snapshot};
use moonbuild::runtest::TestStatistics;
use tracing::info;

use crate::run::PackageFilter;
use moonutil::build_options::TestIndexRange;

use super::{ReplaceableTestResults, TestResultKind};

struct PromotionPlan {
    rerun_filter: PackageFilter,
    snapshot_results: Vec<Arc<TestStatistics>>,
    expect_results: Vec<Arc<TestStatistics>>,
}

impl PromotionPlan {
    fn len(&self) -> usize {
        self.snapshot_results.len() + self.expect_results.len()
    }

    fn rerun_filter(&self) -> &PackageFilter {
        &self.rerun_filter
    }

    fn apply(self, pkg_src: &impl PackageSrcResolver) -> anyhow::Result<PackageFilter> {
        apply_snapshot(
            pkg_src,
            self.snapshot_results.iter().map(|result| result.as_ref()),
        )
        .context("Failed to promote snapshots")?;
        apply_expect(
            pkg_src,
            self.expect_results.iter().map(|result| result.as_ref()),
        )
        .context("Failed to promote expects")?;
        Ok(self.rerun_filter)
    }
}

fn collect_promotions(results: &ReplaceableTestResults) -> anyhow::Result<PromotionPlan> {
    let mut rerun_filter = PackageFilter::default();
    let mut snapshot_results = vec![];
    let mut expect_results = vec![];

    for (target, target_result) in &results.map {
        for (file, results_by_index) in &target_result.map {
            for (index, result) in results_by_index {
                let destination = match result.kind {
                    TestResultKind::SnapshotTestFailed => &mut snapshot_results,
                    TestResultKind::ExpectTestFailed => &mut expect_results,
                    _ => continue,
                };
                rerun_filter.add_one(
                    *target,
                    Some(file),
                    Some(TestIndexRange::from_single(*index)?),
                );
                destination.push(Arc::clone(&result.raw));
            }
        }
    }

    Ok(PromotionPlan {
        rerun_filter,
        snapshot_results,
        expect_results,
    })
}

/// Perform promotion on all test snapshots and expect tests met. Returns
/// the total number of tests promoted, along with a filter indicating which
/// tests needs rerunning.
pub(crate) fn perform_promotion(
    pkg_src: &impl PackageSrcResolver,
    results: &ReplaceableTestResults,
) -> anyhow::Result<(usize, PackageFilter)> {
    let plan = collect_promotions(results)?;
    let count = plan.len();
    let pending_targets = plan.rerun_filter().0.len();
    let rerun_filter = plan.apply(pkg_src)?;
    info!(count, pending_targets, "promoted test results");
    Ok((count, rerun_filter))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use moonbuild::runtest::TestStatistics;
    use moonbuild_rupes_recta::model::{BuildTarget, PackageId, TargetKind};
    use moonutil::test_metadata::MbtTestInfo;

    use super::collect_promotions;
    use crate::run::runtest::{
        ReplaceableTestResults, TargetTestResult, TestCaseResult, TestResultKind,
    };

    fn result(kind: TestResultKind, message: &str) -> TestCaseResult {
        TestCaseResult {
            kind,
            raw: Arc::new(TestStatistics {
                package: "example/pkg".into(),
                filename: "lib.mbt".into(),
                index: "0".into(),
                test_name: "test".into(),
                message: message.into(),
            }),
            meta: MbtTestInfo {
                index: 0,
                func: "test_0".into(),
                name: Some("test".into()),
                line_number: Some(1),
                attrs: vec![],
            },
        }
    }

    #[test]
    fn promotion_plan_tracks_only_updates_and_their_rerun_locations() {
        let target = BuildTarget {
            package: PackageId::default(),
            kind: TargetKind::InlineTest,
        };
        let mut target_results = TargetTestResult::default();
        target_results.add(
            "inline.mbt",
            2,
            result(TestResultKind::ExpectTestFailed, "expect update"),
        );
        target_results.add(
            "snapshot.mbt",
            4,
            result(TestResultKind::SnapshotTestFailed, "snapshot update"),
        );
        target_results.add(
            "ordinary.mbt",
            6,
            result(TestResultKind::Failed, "ordinary failure"),
        );
        let mut results = ReplaceableTestResults::default();
        results.map.insert(target, target_results);

        let plan = collect_promotions(&results).unwrap();

        assert_eq!(plan.len(), 2);
        assert_eq!(plan.expect_results[0].message, "expect update");
        assert_eq!(plan.snapshot_results[0].message, "snapshot update");
        let rerun_filter = plan.rerun_filter();
        let files = rerun_filter.0[&target].as_ref().unwrap();
        assert!(files.0["inline.mbt"].as_ref().unwrap().contains(2));
        assert!(!files.0["inline.mbt"].as_ref().unwrap().contains(3));
        assert!(files.0["snapshot.mbt"].as_ref().unwrap().contains(4));
        assert!(!files.0.contains_key("ordinary.mbt"));
    }
}
