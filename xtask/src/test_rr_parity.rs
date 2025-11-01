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

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Deserialize, Serialize)]
struct TestSuiteEvent {
    #[serde(rename = "type")]
    event_type: String,
    event: String,
    passed: u32,
    failed: u32,
    ignored: u32,
    measured: u32,
    filtered_out: u32,
    exec_time: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct TestEvent {
    #[serde(rename = "type")]
    event_type: String,
    event: String,
    name: String,
}

#[derive(Debug, Clone)]
struct TestStatistics {
    passed: u32,
    failed: u32,
    ignored: u32,
    measured: u32,
    filtered_out: u32,
    exec_time: f64,
}

#[derive(Debug, Clone)]
struct TestResult {
    statistics: TestStatistics,
    failed_tests: Vec<String>,
}

fn check_nightly_toolchain() -> Result<bool> {
    let output = Command::new("cargo")
        .args(["+nightly", "--version"])
        .output()
        .context("Failed to check for nightly toolchain")?;

    Ok(output.status.success())
}

fn run_cargo_test(with_moon_unstable: bool, cargo_args: &[String]) -> Result<TestResult> {
    let mut cmd = Command::new("cargo");
    cmd.args(["+nightly", "test", "--workspace", "--no-fail-fast"]);

    // Add any additional cargo args before the -- separator
    let double_dash = cargo_args.iter().position(|s| *s == "--");
    let (cargo_args, test_args) = match double_dash {
        Some(idx) => (&cargo_args[..idx], &cargo_args[idx + 1..]),
        None => (cargo_args, &[] as &[String]),
    };

    cmd.args(cargo_args);
    cmd.args(["--", "-Z", "unstable-options", "--format", "json"]);
    cmd.args(test_args);

    if with_moon_unstable {
        cmd.env("NEW_MOON", "1");
    }

    let output = cmd
        .output()
        .context("Failed to execute cargo test command")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_test_output(&stdout)
}

fn parse_test_output(output: &str) -> Result<TestResult> {
    let mut statistics = TestStatistics {
        passed: 0,
        failed: 0,
        ignored: 0,
        measured: 0,
        filtered_out: 0,
        exec_time: 0.0,
    };
    let mut failed_tests = Vec::new();

    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }

        // Try to parse as TestSuiteEvent first
        if let Ok(suite_event) = serde_json::from_str::<TestSuiteEvent>(line) {
            if suite_event.event_type == "suite" {
                // Sum all the statistics from multiple suite events
                statistics.passed += suite_event.passed;
                statistics.failed += suite_event.failed;
                statistics.ignored += suite_event.ignored;
                statistics.measured += suite_event.measured;
                statistics.filtered_out += suite_event.filtered_out;
                statistics.exec_time += suite_event.exec_time;
            }
            continue;
        }

        // Try to parse as TestEvent for individual test failures
        if let Ok(test_event) = serde_json::from_str::<TestEvent>(line)
            && test_event.event_type == "test"
            && test_event.event == "failed"
        {
            failed_tests.push(test_event.name);
        }
    }

    Ok(TestResult {
        statistics,
        failed_tests,
    })
}

fn load_baseline(path: &Path) -> Result<BTreeSet<String>> {
    let file = fs::File::open(path)
        .with_context(|| format!("Failed to open baseline file {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut set = BTreeSet::new();
    for line in reader.lines() {
        let line = line?;
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        set.insert(t.to_string());
    }
    Ok(set)
}

const BASELINE_HEADER: &str = "\
# RR-only failing tests baseline
# One fully-qualified test name per line.
# Lines starting with # and blank lines are ignored.
# Generated by: cargo xtask test-rr-parity
# Typical local workflow (compare then refresh):
#   cargo xtask test-rr-parity --compare-baseline xtask/rr_expected_failures.txt --write-baseline xtask/rr_expected_failures.txt
# Note: This list is intended to be checked in and validated by CI on a single platform.
";

fn write_baseline(path: &Path, names: &BTreeSet<String>) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    let mut file = fs::File::create(path)
        .with_context(|| format!("Failed to create baseline file {}", path.display()))?;
    // Write a nice, stable header
    write!(file, "{}", BASELINE_HEADER)?;
    for name in names {
        writeln!(file, "{}", name)?;
    }
    Ok(())
}

// --- helpers to keep parity_test short and readable ---

fn run_suites(cargo_args: &[String]) -> Result<(TestResult, TestResult)> {
    eprintln!("Running legacy tests");
    let legacy = run_cargo_test(false, cargo_args).context("Error running legacy tests")?;

    eprintln!("Running RR tests");
    let rr = run_cargo_test(true, cargo_args).context("Error running RR tests")?;

    Ok((legacy, rr))
}

fn run_rr_multiple(cargo_args: &[String], runs: usize) -> Result<Vec<TestResult>> {
    let mut results = Vec::with_capacity(runs);
    for idx in 0..runs {
        if runs == 1 {
            eprintln!("Running RR tests");
        } else {
            eprintln!("Running RR tests (iteration {}/{})", idx + 1, runs);
        }
        let rr = run_cargo_test(true, cargo_args).with_context(|| {
            format!("Error running RR tests (iteration {} of {})", idx + 1, runs)
        })?;
        results.push(rr);
    }
    Ok(results)
}

fn rr_only_failures(without_rr: &TestResult, with_rr: &TestResult) -> BTreeSet<String> {
    let without_rr_failed: HashSet<_> = without_rr.failed_tests.iter().collect();
    let with_rr_failed: HashSet<_> = with_rr.failed_tests.iter().collect();
    with_rr_failed
        .difference(&without_rr_failed)
        .map(|s| (*s).clone())
        .collect()
}

fn print_rr_only(rr_only: &BTreeSet<String>) {
    if rr_only.is_empty() {
        return;
    }
    println!("Tests that Rupes Recta fails:");
    for test in rr_only {
        println!("  {}", test);
    }
    println!();
}

fn print_rr_stable(rr_only: &BTreeSet<String>, runs: usize) {
    if rr_only.is_empty() {
        return;
    }
    println!("RR-only failures consistent across {} runs:", runs);
    for test in rr_only {
        println!("  {}", test);
    }
    println!();
}

fn print_rr_unstable(unstable: &BTreeSet<String>, counts: &BTreeMap<String, usize>, runs: usize) {
    if unstable.is_empty() {
        return;
    }
    println!("RR-only unstable failures across {} runs:", runs);
    for test in unstable {
        let seen = counts.get(test).copied().unwrap_or(0);
        println!("  {} ({}/{})", test, seen, runs);
    }
    println!();
}

fn print_stats(without_rr: &TestResult, with_rr: &TestResult) {
    println!(
        "Legacy: passed={}, failed={}, ignored={}, measured={}, filtered_out={}, exec_time={:.3}s",
        without_rr.statistics.passed,
        without_rr.statistics.failed,
        without_rr.statistics.ignored,
        without_rr.statistics.measured,
        without_rr.statistics.filtered_out,
        without_rr.statistics.exec_time
    );
    println!(
        "RR:     passed={}, failed={}, ignored={}, measured={}, filtered_out={}, exec_time={:.3}s",
        with_rr.statistics.passed,
        with_rr.statistics.failed,
        with_rr.statistics.ignored,
        with_rr.statistics.measured,
        with_rr.statistics.filtered_out,
        with_rr.statistics.exec_time
    );
}

fn is_github_actions() -> bool {
    matches!(env::var("GITHUB_ACTIONS"), Ok(val) if val == "true")
}

fn gha_warning(msg: &str) {
    if is_github_actions() {
        println!("::warning::{}", msg);
    } else {
        eprintln!("WARNING: {}", msg);
    }
}

fn compare_baseline(rr_only: &BTreeSet<String>, path: &Path) -> Result<(bool, bool)> {
    let baseline = load_baseline(path)?;
    let new_failures: BTreeSet<String> = rr_only.difference(&baseline).cloned().collect();
    let fixed_failures: BTreeSet<String> = baseline.difference(rr_only).cloned().collect();

    if !new_failures.is_empty() {
        println!("New RR-only failures (not in baseline):");
        for name in &new_failures {
            println!("  {}", name);
        }
        println!();
    }

    if !fixed_failures.is_empty() {
        println!("Fixed RR-only failures (in baseline, now passing):");
        for name in &fixed_failures {
            println!("  {}", name);
        }
        println!();
    }

    Ok((!new_failures.is_empty(), !fixed_failures.is_empty()))
}

pub fn parity_test(
    compare_path: Option<&Path>,
    write_path: Option<&Path>,
    rr_runs: usize,
    cargo_args: &[String],
) -> i32 {
    if !check_nightly_toolchain().unwrap() {
        eprintln!(
            "Nightly toolchain not found. Please install with: rustup toolchain install nightly"
        );
        eprintln!("Note: Nightly toolchain is required for parsing test outputs.");
        return 1;
    }

    let rr_runs = rr_runs.max(1);
    let baseline_set;
    let has_parity;

    if rr_runs == 1 {
        let (without_rr, with_rr) = match run_suites(cargo_args) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("{e:#}");
                return 1;
            }
        };

        let rr_only = rr_only_failures(&without_rr, &with_rr);
        print_rr_only(&rr_only);
        print_stats(&without_rr, &with_rr);

        baseline_set = rr_only.clone();
        has_parity = without_rr.statistics.passed == with_rr.statistics.passed
            && without_rr.statistics.failed == with_rr.statistics.failed
            && rr_only.is_empty();
    } else {
        eprintln!("Running legacy tests");
        let legacy = match run_cargo_test(false, cargo_args).context("Error running legacy tests") {
            Ok(v) => v,
            Err(e) => {
                eprintln!("{e:#}");
                return 1;
            }
        };

        let rr_results = match run_rr_multiple(cargo_args, rr_runs) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("{e:#}");
                return 1;
            }
        };

        let rr_only_sets: Vec<BTreeSet<String>> = rr_results
            .iter()
            .map(|rr| rr_only_failures(&legacy, rr))
            .collect();

        let mut counts = BTreeMap::<String, usize>::new();
        let mut union_all = BTreeSet::<String>::new();
        let mut intersection: Option<BTreeSet<String>> = None;

        for set in &rr_only_sets {
            for name in set {
                *counts.entry(name.clone()).or_default() += 1;
                union_all.insert(name.clone());
            }
            let new_intersection = match &intersection {
                None => set.clone(),
                Some(current) => current.intersection(set).cloned().collect(),
            };
            intersection = Some(new_intersection);
        }

        let stable = intersection.unwrap_or_default();
        let unstable: BTreeSet<String> = union_all.difference(&stable).cloned().collect();

        print_rr_stable(&stable, rr_results.len());
        print_rr_unstable(&unstable, &counts, rr_results.len());

        if let Some(first_rr) = rr_results.first() {
            print_stats(&legacy, first_rr);
        }

        if !rr_results.is_empty() {
            let total_exec_time: f64 = rr_results.iter().map(|rr| rr.statistics.exec_time).sum();
            let runs = rr_results.len();
            println!(
                "RR rerun summary: runs={}, stable_failures={}, unstable_failures={}, total_exec_time={:.3}s, avg_exec_time={:.3}s",
                runs,
                stable.len(),
                unstable.len(),
                total_exec_time,
                total_exec_time / runs as f64
            );
        }

        baseline_set = union_all;
        has_parity = baseline_set.is_empty();
    }

    if compare_path.is_some() || write_path.is_some() {
        let mut has_new = false;
        let mut has_fixed = false;

        if let Some(path) = compare_path {
            match compare_baseline(&baseline_set, path) {
                Ok((new_found, fixed_found)) => {
                    has_new = new_found;
                    has_fixed = fixed_found;
                }
                Err(e) => {
                    eprintln!("Failed to compare baseline '{}': {}", path.display(), e);
                    return 1;
                }
            }
        }

        if has_fixed {
            gha_warning(
                "RR-only failures fixed since baseline; consider updating the baseline with --write-baseline.",
            );
        }

        if let Some(path) = write_path
            && let Err(e) = write_baseline(path, &baseline_set)
        {
            eprintln!("Failed to write baseline '{}': {}", path.display(), e);
            return 1;
        }

        if compare_path.is_some() && has_new {
            1
        } else {
            0
        }
    } else if has_parity {
        0
    } else {
        1
    }
}
