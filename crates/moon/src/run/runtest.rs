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

/*!
    Run tests and interpret the results.

    # Workflow overview

    ## A normal test workflow

    1. Build test executable (using [`crate::rr_build`])
    2. Run tests using [`run_tests`]
    3. Interpret and print the test results using [`ReplaceableTestResults::print_result`]

    ## Tests with snapshot promotion

    Currently, snapshot promotion of tests require rerunning the test suite
    after the values are promoted. The workflow is as follows:

    1. Build test executable as usual
    2. Run first test pass as usual
    3. After the initial test results are generated, pass it to the following:
    4. Loop with the result of the last test run, with capped iteration count:
        1. Scan the test result for any files to promote. If none, break.
        2. Perform the promotion for the given files, and keep track of which
           tests (package, file, index) are promoted.
        3. Rerun the build of the affected packages.
        4. Rerun the affected tests using the filter generated from the tracking.
        5. Merge the new result with the main one, while keeping a copy locally
           for the next iteration.
    5. After the loop, print the final result as usual.

    ## Future improvements

    There is an ongoing discussion about the snapshot promotion behavior. If we
    can change the snapshot promotion approach to single-pass, we will be able
    to greatly simplify the snapshot promotion routine. In particular, we can
    remove the iterative running behavior, have real time test result printing,
    and remove the [`ReplaceableTestResults`] type altogether. (known issue: the
    line numbers will still be stale, but we can mitigate it locally)

    Check the discussion at [core#2684](https://github.com/moonbitlang/core/issues/2684).
*/

mod filter;
mod promotion;

use std::{collections::HashMap, path::Path, sync::Arc};

use anyhow::Context;
use indexmap::IndexMap;
use log::warn;
use moonbuild::{
    benchmark::{render_batch_bench_summary, BATCHBENCH},
    entry::{CompactTestFormatter, TestArgs},
    expect::{
        render_expect_fail, render_snapshot_fail, ERROR, EXPECT_FAILED, RUNTIME_ERROR,
        SNAPSHOT_TESTING,
    },
    runtest::TestStatistics,
    section_capture::SectionCapture,
};
use moonbuild_rupes_recta::model::{BuildPlanNode, BuildTarget};
use moonutil::common::{
    MbtTestInfo, MooncGenTestInfo, MOON_COVERAGE_DELIMITER_BEGIN, MOON_COVERAGE_DELIMITER_END,
    MOON_TEST_DELIMITER_BEGIN, MOON_TEST_DELIMITER_END,
};
use tokio::runtime::Runtime;

use crate::{rr_build::BuildMeta, run::default_rt};

pub use filter::TestFilter;
pub use promotion::perform_promotion;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestResultKind {
    Passed,
    ExpectTestFailed,
    SnapshotTestFailed,
    RuntimeError,
    ExpectPanic,
    Failed,
}

#[derive(Debug, Clone)]
struct TestCaseResult {
    kind: TestResultKind,
    raw: Arc<TestStatistics>,
    // The metadata structure is read per-executable, so we better own it.
    // Known issue: line numbers can be stale if we are promoting tests
    meta: MbtTestInfo,
}

impl TestCaseResult {
    pub fn passed(&self) -> bool {
        matches!(self.kind, TestResultKind::Passed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TestIndex {
    /// A regular test block, i.e. `test { ... }`
    Regular(u32),
    /// A doctest block after `///`
    DocTest(u32),
}

impl TestIndex {
    /// Extract the value of the index, when there is no ambiguity.
    pub fn value(self) -> u32 {
        match self {
            TestIndex::Regular(v) => v,
            TestIndex::DocTest(v) => v,
        }
    }
}

/// Run the tests compiled in this session. Does **not** print or update
/// snapshots.
///
/// An external driver should check the results for reruns. See [module-level
/// docs](crate::run::runtest) for more information about the workflow.
pub fn run_tests(
    build_meta: &BuildMeta,
    target_dir: &Path,
    filter: &TestFilter,
) -> anyhow::Result<ReplaceableTestResults> {
    // Gathering artifacts
    let executables = gather_tests(build_meta);

    let rt = default_rt().context("Failed to create runtime")?;
    let mut stats = ReplaceableTestResults::default();
    for r in executables {
        let res = run_one_test_executable(build_meta, &rt, target_dir, &r, filter)?;
        stats.merge_with_target(r.target, res);
    }

    Ok(stats)
}

#[derive(derive_builder::Builder)]
#[builder(derive(Debug))]
struct TestExecutableToRun<'a> {
    target: BuildTarget,
    executable: &'a Path,
    meta: &'a Path,
}

/// A container of test results corresponding to each test artifact, and
/// can be replaced by later test runs upon test result updates.
#[derive(Default, Debug)]
pub struct ReplaceableTestResults {
    map: IndexMap<BuildTarget, TargetTestResult>,
}

/// The test result for a single build target
#[derive(Default, Clone, Debug)]
struct TargetTestResult {
    /// `Map<file, Map<index, result>>`
    map: IndexMap<String, IndexMap<u32, TestCaseResult>>,
}

#[derive(Default, Clone, Debug)]
pub struct TestSummary {
    pub total: usize,
    pub passed: usize,
}

impl ReplaceableTestResults {
    fn merge_with_target(&mut self, target: BuildTarget, result: TargetTestResult) {
        let entry = self.map.entry(target).or_default();
        for (file, file_map) in result.map {
            let file_entry = entry.map.entry(file).or_default();
            for (index, case) in file_map {
                file_entry.insert(index, case);
            }
        }
    }

    #[allow(unused)] // test promotion will use it
    pub fn merge(&mut self, other: &ReplaceableTestResults) {
        for (target, result) in &other.map {
            // inefficient but should not be bottleneck
            self.merge_with_target(*target, result.clone());
        }
    }

    pub fn print_result(&self, meta: &BuildMeta, verbose: bool) {
        for (target, result) in &self.map {
            let module_name = meta
                .resolve_output
                .pkg_dirs
                .get_package(target.package)
                .fqn
                .module()
                .name()
                .to_string();
            for file_map in result.map.values() {
                for res in file_map.values() {
                    print_test_result(res, &module_name, verbose);
                }
            }
        }
    }

    pub fn summary(&self) -> TestSummary {
        let mut total = 0;
        let mut passed = 0;
        for result in self.map.values() {
            for file_map in result.map.values() {
                total += file_map.len();
                passed += file_map.values().filter(|r| r.passed()).count();
            }
        }
        TestSummary { total, passed }
    }
}

impl TargetTestResult {
    pub fn add(&mut self, file: &str, index: u32, result: TestCaseResult) {
        match self.map.get_mut(file) {
            Some(v) => {
                v.insert(index, result);
            }
            None => {
                // We're not using Map::entry(K) because String is not Copy, and
                // it will allocate even if the key is found
                let mut m = IndexMap::new();
                m.insert(index, result);
                self.map.insert(file.to_string(), m);
            }
        }
    }
}

/// Gather tests executables from the build metadata.
fn gather_tests(build_meta: &BuildMeta) -> Vec<TestExecutableToRun<'_>> {
    let mut pending = HashMap::new();
    let mut results = vec![];

    for (node, artifacts) in &build_meta.artifacts {
        let target = node
            .extract_target()
            .expect("All artifacts of tests should contain a build target");

        let working = pending.entry(target).or_insert_with(|| {
            let mut res = TestExecutableToRunBuilder::create_empty();
            res.target(target);
            res
        });

        // FIXME: artifact index relies on implementation of append_artifact_of
        match node {
            BuildPlanNode::MakeExecutable(_) => working.executable(&artifacts.artifacts[0]),
            BuildPlanNode::GenerateTestInfo(_) => working.meta(&artifacts.artifacts[1]),
            _ => panic!("Unexpected artifact for test: {:?}", artifacts.node),
        };

        if let Ok(tgt) = working.build() {
            pending.remove(&target);
            results.push(tgt);
        }
    }

    // Sort by artifact path -- this is the same as legacy behavior
    results.sort_by_key(|v| v.executable);

    assert_eq!(
        pending.len(),
        0,
        "Some test targets are missing artifacts: {:?}",
        &pending
    );

    results
}

fn run_one_test_executable(
    build_meta: &BuildMeta,
    rt: &Runtime, // FIXME: parallel execution
    target_dir: &Path,
    test: &TestExecutableToRun,
    filter: &TestFilter,
) -> Result<TargetTestResult, anyhow::Error> {
    let (included, file_filt) = filter.check_package(test.target);
    if !included {
        return Ok(TargetTestResult::default());
    }

    let fqn = build_meta.resolve_output.pkg_dirs.fqn(test.target.package);
    let pkgname = fqn.to_string();

    // Parse test metadata
    let meta = std::fs::File::open(test.meta).context("Failed to open test metadata")?;
    let meta: MooncGenTestInfo = serde_json_lenient::from_reader(meta)
        .with_context(|| format!("Failed to parse test metadata at {}", test.meta.display()))?;

    let mut test_args = TestArgs {
        package: pkgname,
        file_and_index: vec![],
    };

    filter::apply_filter(file_filt, &meta, &mut test_args.file_and_index);

    let cmd =
        crate::run::command_for(build_meta.target_backend, test.executable, Some(&test_args))?;
    let mut cov_cap = mk_coverage_capture();
    let mut test_cap = make_test_capture();

    rt.block_on(crate::run::run(
        &mut [&mut cov_cap, &mut test_cap],
        false,
        cmd.command,
    ))
    .with_context(|| format!("Failed to run test for {fqn} {:?}", test.target.kind))?;

    handle_finished_coverage(target_dir, cov_cap)?;

    parse_test_results(meta, test_cap).with_context(|| {
        format!(
            "Failed to parse test results for {fqn} {:?}",
            test.target.kind
        )
    })
}

fn mk_coverage_capture() -> SectionCapture<'static> {
    SectionCapture::new(
        MOON_COVERAGE_DELIMITER_BEGIN,
        MOON_COVERAGE_DELIMITER_END,
        true,
    )
}

fn make_test_capture() -> SectionCapture<'static> {
    SectionCapture::new(MOON_TEST_DELIMITER_BEGIN, MOON_TEST_DELIMITER_END, false)
}

fn handle_finished_coverage(target_dir: &Path, cap: SectionCapture) -> anyhow::Result<()> {
    if let Some(coverage_output) = cap.finish() {
        let time = chrono::Local::now().timestamp_micros();
        let rnd = rand::random::<u32>();

        // Note: maybe we should choose another filename?
        let filename = target_dir.join(format!("moonbit_coverage_{time}_{rnd:08x}.txt"));

        std::fs::write(&filename, coverage_output).context(format!(
            "failed to write coverage result to {}",
            filename.to_string_lossy()
        ))?;
    }
    Ok(())
}

fn parse_test_results(
    meta: MooncGenTestInfo,
    cap: SectionCapture,
) -> anyhow::Result<TargetTestResult> {
    let Some(s) = cap.finish() else {
        return Ok(TargetTestResult::default());
    };

    // Create a map to repopulate test names
    // map<filename, map<index, test_case_name>>
    let mut test_name_map: HashMap<String, HashMap<u32, MbtTestInfo>> = HashMap::new();
    for (file, tests) in [
        meta.no_args_tests,
        meta.with_args_tests,
        meta.with_bench_args_tests,
        meta.async_tests,
    ]
    .into_iter()
    .flatten()
    {
        let file_map = test_name_map.entry(file).or_default();
        for t in tests {
            file_map.insert(t.index, t);
        }
    }

    // Actual handling of each test case result
    let mut res = TargetTestResult::default();
    for line in s.lines() {
        if line.is_empty() {
            continue;
        }

        let stat: TestStatistics = serde_json_lenient::from_str(line)
            .with_context(|| format!("Failed to parse test summary: {line}"))?;
        let stat = Arc::new(stat);

        // Repopulate name.
        // The test name in stat may be different from that in source code,
        // due to how cases like panics are handled, causing later handling to
        // deviate from what we expect. Here, we fetch the name from the
        // metadata to avoid the problem.
        let index = stat.index.parse::<u32>().with_context(|| {
            format!(
                "Failed to parse test index {} for {}",
                stat.index, stat.test_name
            )
        })?;
        let meta = test_name_map
            .get_mut(stat.filename.as_str())
            .and_then(|v| v.remove(&index));
        let Some(meta) = meta else {
            warn!(
                "Failed to find test metadata for {} index {}",
                stat.filename, stat.index
            );
            continue;
        };
        // .with_context(|| {
        //     format!(
        //         "Failed to find test metadata for {} index {}",
        //         stat.filename, stat.index
        //     )
        // })?;
        let name = meta.name.as_ref().unwrap_or(&stat.test_name);
        let result_kind = parse_one_test_result(&stat, name)?;
        let case_result = TestCaseResult {
            kind: result_kind,
            raw: Arc::clone(&stat),
            meta,
        };
        res.add(&stat.filename, index, case_result);
    }

    Ok(res)
}

fn parse_one_test_result(
    result: &TestStatistics,
    test_name: &str,
) -> anyhow::Result<TestResultKind> {
    use TestResultKind::*;

    let res = if test_name.starts_with("panic") {
        // For whatever reason, any message in the result is viewed as the code
        // successfully panicked.
        // FIXME: random prefix matching is bad, should use a proper attribute or something in code
        if result.message.is_empty() {
            ExpectPanic
        } else {
            Passed
        }
    } else if result.message.starts_with(EXPECT_FAILED) {
        ExpectTestFailed
    } else if result.message.starts_with(SNAPSHOT_TESTING) {
        // FIXME: file access HERE?!
        if moonbuild::expect::snapshot_eq(&result.message)
            .with_context(|| format!("Failed to read snapshot for {}", result.test_name))?
        {
            Passed
        } else {
            SnapshotTestFailed
        }
    } else if result.message.starts_with(RUNTIME_ERROR) || result.message.starts_with(ERROR) {
        RuntimeError
    } else if result.message.starts_with(BATCHBENCH) || result.message.is_empty() {
        Passed
    } else {
        Failed
    };
    Ok(res)
}

fn print_test_result(res: &TestCaseResult, module_name: &str, verbose: bool) {
    let message = &res.raw.message;
    let formatter = CompactTestFormatter::new(module_name, &res.raw, Some(&res.meta));

    match res.kind {
        TestResultKind::Passed if !verbose => {}
        TestResultKind::Passed => {
            if message.starts_with(BATCHBENCH) {
                let _ = formatter.write_bench(&mut std::io::stdout());
                println!();
                render_batch_bench_summary(message);
            } else {
                let _ = formatter.write_success(&mut std::io::stdout());
                println!();
            }
        }

        TestResultKind::Failed | TestResultKind::RuntimeError => {
            if message.is_empty() {
                let _ = formatter.write_failure(&mut std::io::stdout());
            } else {
                let _ = formatter.write_failure_with_message(&mut std::io::stdout(), message);
            }
            println!();
        }
        TestResultKind::ExpectTestFailed => {
            let _ = formatter.write_failure(&mut std::io::stdout());
            println!();
            let _ = render_expect_fail(message);
        }
        TestResultKind::SnapshotTestFailed => {
            let _ = formatter.write_failure(&mut std::io::stdout());
            println!();
            let _ = render_snapshot_fail(message);
        }
        TestResultKind::ExpectPanic => {
            // For panic tests, success means we got a message (panic occurred)
            if !message.is_empty() {
                let _ = formatter.write_success(&mut std::io::stdout());
            } else {
                let _ = formatter.write_failure(&mut std::io::stdout());
            }
            println!();
        }
    }
}
