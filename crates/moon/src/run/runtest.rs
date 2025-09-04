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

//! Run tests and interpret the results

use std::{collections::HashMap, path::Path};

use anyhow::Context;
use moonbuild::{
    benchmark::BATCHBENCH,
    entry::TestArgs,
    expect::{ERROR, EXPECT_FAILED, RUNTIME_ERROR, SNAPSHOT_TESTING},
    runtest::TestStatistics,
    section_capture::SectionCapture,
};
use moonbuild_rupes_recta::model::{BuildPlanNode, BuildTarget, TargetKind};
use moonutil::common::{
    MooncGenTestInfo, MOON_COVERAGE_DELIMITER_BEGIN, MOON_COVERAGE_DELIMITER_END,
    MOON_TEST_DELIMITER_BEGIN, MOON_TEST_DELIMITER_END,
};
use tokio::runtime::Runtime;

use crate::{rr_build::BuildMeta, run::default_rt};

enum TestResultKind {
    Passed,
    ExpectTestFailed,
    SnapshotTestFailed,
    RuntimeError,
    ExpectPanic,
    Failed,
}

struct TestCaseResult {
    kind: TestResultKind,
    _raw: TestStatistics, // will use later for test promotion
}

impl TestCaseResult {
    pub fn passed(&self) -> bool {
        matches!(self.kind, TestResultKind::Passed)
    }
}

#[derive(Debug, Default)]
pub struct TestResult {
    pub total: usize,
    pub passed: usize,
}

impl TestResult {
    pub fn merge(&mut self, other: TestResult) {
        self.total += other.total;
        self.passed += other.passed;
    }

    pub fn passed(&self) -> bool {
        self.total == self.passed
    }
}

pub struct TestFilter {
    pub file: Option<String>,
    pub index: Option<TestIndex>,
}

pub enum TestIndex {
    /// A regular test block, i.e. `test { ... }`
    Regular(u32),
    /// A doctest block after `///`
    DocTest(u32),
}

/// Run the tests compiled in this session.
pub fn run_tests(
    build_meta: &BuildMeta,
    target_dir: &Path,
    filter: &TestFilter,
) -> anyhow::Result<TestResult> {
    // Gathering artifacts
    let results = gather_tests(build_meta);

    let rt = default_rt().context("Failed to create runtime")?;
    let mut stats = TestResult::default();
    for r in results {
        let res = run_one_test_executable(build_meta, &rt, target_dir, &r, filter)?;
        stats.merge(res);
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

/// Gather tests executables from the build metadata.
fn gather_tests(build_meta: &BuildMeta) -> Vec<TestExecutableToRun<'_>> {
    let mut pending = HashMap::new();
    let mut results = vec![];

    for artifacts in &build_meta.artifacts {
        let target = artifacts
            .node
            .extract_target()
            .expect("All artifacts of tests should contain a build target");

        let working = pending.entry(target).or_insert_with(|| {
            let mut res = TestExecutableToRunBuilder::create_empty();
            res.target(target);
            res
        });

        // FIXME: artifact index relies on implementation of append_artifact_of
        match artifacts.node {
            BuildPlanNode::MakeExecutable(_) => working.executable(&artifacts.artifacts[0]),
            BuildPlanNode::GenerateTestInfo(_) => working.meta(&artifacts.artifacts[1]),
            _ => panic!("Unexpected artifact for test: {:?}", artifacts.node),
        };

        if let Ok(tgt) = working.build() {
            pending.remove(&target);
            results.push(tgt);
        }
    }

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
) -> Result<TestResult, anyhow::Error> {
    let pkgname = build_meta
        .resolve_output
        .pkg_dirs
        .get_package(test.target.package)
        .fqn
        .to_string();

    // Package filtering should already be done when building test executables

    // Parse test metadata
    let meta = std::fs::File::open(test.meta).context("Failed to open test metadata")?;
    let meta: MooncGenTestInfo = serde_json_lenient::from_reader(meta)
        .with_context(|| format!("Failed to parse test metadata at {}", test.meta.display()))?;

    let mut test_args = TestArgs {
        package: pkgname,
        file_and_index: vec![],
    };

    for lists in [
        &meta.no_args_tests,
        &meta.with_args_tests,
        &meta.with_bench_args_tests,
    ] {
        for (filename, test_infos) in lists {
            // Filter by file name
            if let Some(ffile) = &filter.file {
                if ffile != filename {
                    continue;
                }
            }
            let is_bbtest_file = filename.ends_with("_test.mbt"); // FIXME: heuristic

            // Filter by index
            match filter.index {
                // No filter -- run all tests in the file
                None => {
                    if !test_infos.is_empty() {
                        let max_index = test_infos.iter().map(|t| t.index).max().unwrap_or(0);
                        test_args
                            .file_and_index
                            .push((filename.to_string(), 0..(max_index + 1)));
                    }
                }
                // Regular tests
                Some(TestIndex::Regular(index)) => {
                    if (test.target.kind != TargetKind::BlackboxTest || is_bbtest_file)
                        && test_infos.iter().any(|t| t.index == index)
                    {
                        test_args
                            .file_and_index
                            .push((filename.to_string(), index..(index + 1)));
                    }
                }
                // Doctests -- specifically for test blocks in
                // non-black-box-test files in black box test build targets.
                Some(TestIndex::DocTest(index)) => {
                    if test.target.kind == TargetKind::BlackboxTest
                        && !is_bbtest_file
                        && test_infos.iter().any(|t| t.index == index)
                    {
                        test_args
                            .file_and_index
                            .push((filename.to_string(), index..(index + 1)));
                    }
                }
            }
        }
    }

    let cmd =
        crate::run::command_for(build_meta.target_backend, test.executable, Some(&test_args))?;
    let mut cov_cap = mk_coverage_capture();
    let mut test_cap = make_test_capture();

    rt.block_on(crate::run::run(
        &mut [&mut cov_cap, &mut test_cap],
        false,
        cmd.command,
    ))
    .context("Failed to run test")?;

    handle_finished_coverage(target_dir, cov_cap)?;
    let stats = get_test_statistics(&meta, test_cap)?;

    // TODO: update snapshots and expect tests

    let total_count = stats.len();
    let passed_count = stats.iter().filter(|x| x.passed()).count();

    Ok(TestResult {
        total: total_count,
        passed: passed_count,
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

fn get_test_statistics(
    meta: &MooncGenTestInfo,
    cap: SectionCapture,
) -> anyhow::Result<Vec<TestCaseResult>> {
    let Some(s) = cap.finish() else {
        return Ok(vec![]);
    };

    // Create a map to repopulate test names
    // map<filename, map<index, test_case_name>>
    let mut test_name_map: HashMap<&str, HashMap<u32, Option<&str>>> = HashMap::new();
    for test_list in [
        &meta.no_args_tests,
        &meta.with_args_tests,
        &meta.with_bench_args_tests,
    ] {
        for (file, tests) in test_list {
            let file_map = test_name_map.entry(file.as_str()).or_default();
            for t in tests {
                file_map.insert(t.index, t.name.as_deref());
            }
        }
    }

    // Actual handling of each test case result
    let mut res = vec![];
    for line in s.lines() {
        if line.is_empty() {
            continue;
        }

        let stat: TestStatistics = serde_json_lenient::from_str(line)
            .with_context(|| format!("Failed to parse test summary: {line}"))?;

        // Repopulate name.
        // The test name in stat may be different from that in source code,
        // due to how cases like panics are handled, causing later handling to
        // deviate from what we expect. Here, we fetch the name from the
        // metadata to avoid the problem.
        let name = test_name_map
            .get(stat.filename.as_str())
            .and_then(|v| v.get(&stat.index.parse::<u32>().ok()?).cloned())
            .flatten()
            .unwrap_or(&stat.test_name);

        let result_kind = parse_one_test_result(&stat, name)?;
        res.push(TestCaseResult {
            kind: result_kind,
            _raw: stat,
        });
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
