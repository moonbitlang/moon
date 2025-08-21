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
use std::collections::HashSet;
use std::process::Command;

const MOON_UNSTABLE_RR: &str = "rupes_recta";

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
    cmd.args(cargo_args);

    cmd.args(["--", "-Z", "unstable-options", "--format", "json"]);

    if with_moon_unstable {
        cmd.env("MOON_UNSTABLE", MOON_UNSTABLE_RR);
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
        if let Ok(test_event) = serde_json::from_str::<TestEvent>(line) {
            if test_event.event_type == "test" && test_event.event == "failed" {
                failed_tests.push(test_event.name);
            }
        }
    }

    Ok(TestResult {
        statistics,
        failed_tests,
    })
}

pub fn parity_test(cargo_args: &[String]) -> i32 {
    // Check for nightly toolchain first
    if !check_nightly_toolchain().unwrap() {
        eprintln!(
            "Nightly toolchain not found. Please install with: rustup toolchain install nightly"
        );
        eprintln!("Note: Nightly toolchain is required for parsing test outputs.");
        return 1;
    }

    eprintln!("Running legacy tests");
    let without_rr = match run_cargo_test(false, cargo_args) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("Error running legacy tests: {}", e);
            return 1;
        }
    };

    eprintln!("Running RR tests");
    let with_rr = match run_cargo_test(true, cargo_args) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("Error running RR tests: {}", e);
            return 1;
        }
    };

    // Analyze failed test differences
    let without_rr_failed: HashSet<_> = without_rr.failed_tests.iter().collect();
    let with_rr_failed: HashSet<_> = with_rr.failed_tests.iter().collect();

    let mut only_failed_with_rr: Vec<_> = with_rr_failed.difference(&without_rr_failed).collect();

    // Print tests that failed in RR but not in legacy
    if !only_failed_with_rr.is_empty() {
        println!("Tests that Rupes Recta fails:");
        only_failed_with_rr.sort();
        for test in &only_failed_with_rr {
            println!("  {}", test);
        }
        println!();
    }

    // Print test statistics
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

    // Determine if there's parity
    let has_parity = without_rr.statistics.passed == with_rr.statistics.passed
        && without_rr.statistics.failed == with_rr.statistics.failed
        && only_failed_with_rr.is_empty();

    if has_parity {
        0
    } else {
        1
    }
}
