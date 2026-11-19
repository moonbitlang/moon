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

//! Render test results and benchmark summaries for the Moon CLI.

use std::io::Write;

use crate::test_metadata::MbtTestInfo;
use colored::Colorize;
use moonbuild_rupes_recta::discover::DiscoverResult;

use super::expect::{render_expect_fail, render_snapshot_fail};
use super::{TestCaseResult, TestResultKind, TestStatistics};

pub(super) const BATCHBENCH: &str = "@BATCH_BENCH ";

pub(super) fn print_test_result(
    res: &TestCaseResult,
    module_name: &str,
    verbose: bool,
    json: bool,
    pkg_src: &DiscoverResult,
) {
    if json {
        print_test_result_json(res);
    } else {
        print_test_result_normal(res, module_name, verbose, pkg_src);
    }
}

fn print_test_result_json(res: &TestCaseResult) {
    use TestResultKind::*;

    match res.kind {
        Passed => {
            // In JSON mode, do not emit anything for successful tests.
            // If verbose success output is desired, callers should disable json.
        }
        Failed | RuntimeError | ExpectTestFailed | SnapshotTestFailed | ExpectPanic => {
            // Repopulate test_name from metadata when available
            let test_name = res
                .meta
                .name
                .as_ref()
                .map(|s| s.to_string())
                .unwrap_or_else(|| res.raw.test_name.clone());

            // Normalize message: for ExpectPanic with empty message, align with legacy output
            let mut message = res.raw.message.clone();
            if matches!(res.kind, ExpectPanic) && message.is_empty() {
                message = "panic is expected".to_string();
            }

            let obj = TestStatistics {
                package: res.raw.package.clone(),
                filename: res.raw.filename.clone(),
                index: res.raw.index.clone(),
                test_name,
                message,
            };

            // Print compact JSON line
            println!("{}", serde_json_lenient::to_string(&obj).unwrap());
        }
    }
}

fn print_test_result_normal(
    res: &TestCaseResult,
    module_name: &str,
    verbose: bool,
    pkg_src: &DiscoverResult,
) {
    let output = &res.raw;
    let message = &output.message;
    let formatter = CompactTestFormatter::new(module_name, &res.raw, Some(&res.meta));

    match res.kind {
        TestResultKind::Passed => {
            if message.starts_with(BATCHBENCH) {
                let _ = formatter.write_bench(&mut std::io::stdout());
                println!();
                render_batch_bench_summary(message);
            } else if verbose {
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
            let _ = render_expect_fail(pkg_src, output);
        }
        TestResultKind::SnapshotTestFailed => {
            let _ = formatter.write_failure(&mut std::io::stdout());
            println!();
            let _ = render_snapshot_fail(pkg_src, output);
        }
        TestResultKind::ExpectPanic => {
            let _ =
                formatter.write_failure_with_message(&mut std::io::stdout(), "panic is expected");
            println!();
        }
    }
}

/// Generates compact test output like: `[moontest/lib] my_test.mbt:25 "read should succeed" ok`
struct CompactTestFormatter<'a> {
    module_name: &'a str,
    stats: &'a TestStatistics,
    test_info: Option<&'a MbtTestInfo>,
}

impl<'a> CompactTestFormatter<'a> {
    fn new(
        module_name: &'a str,
        stats: &'a TestStatistics,
        test_info: Option<&'a MbtTestInfo>,
    ) -> Self {
        Self {
            module_name,
            stats,
            test_info,
        }
    }

    fn write_test_identifier<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        if let Some(info) = self.test_info {
            if let Some(name) = info.name.as_ref().filter(|n| !n.is_empty()) {
                write!(w, "{:?}", name)
            } else {
                write!(w, "#{}", info.index)
            }
        } else if !self.stats.test_name.is_empty() {
            write!(w, "{:?}", self.stats.test_name)
        } else {
            write!(w, "#{}", self.stats.index)
        }
    }

    fn write_common_prefix<W: Write>(&self, is_bench: bool, w: &mut W) -> std::io::Result<()> {
        // Try to strip the module prefix from the package name for brevity of output
        let stripped = self
            .stats
            .package
            .strip_prefix(self.module_name)
            .map(|x| x.strip_prefix('/').unwrap_or(x));
        // If we have stripped result, this is a local package and we print the module name only
        if stripped.is_some() {
            write!(w, "[{}] ", self.module_name)?;
        } else {
            write!(w, "[{}] ", self.stats.package)?;
        }
        if is_bench {
            write!(w, "bench ")?;
        } else {
            write!(w, "test ")?;
        }
        if let Some(subpackage) = stripped
            && !subpackage.is_empty()
        {
            write!(w, "{}/", subpackage)?;
        }
        write!(w, "{}", self.stats.filename)?;

        let line_number = self.test_info.and_then(|info| info.line_number);
        if let Some(line_num) = line_number {
            write!(w, ":{}", line_num)?;
            write!(w, " (")?;
            self.write_test_identifier(w)?;
            write!(w, ")")
        } else {
            write!(w, " ")?;
            self.write_test_identifier(w)
        }
    }

    fn write_success<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        self.write_common_prefix(false, w)?;
        write!(w, " {}", "ok".green().bold())
    }

    fn write_failure<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        self.write_common_prefix(false, w)?;
        write!(w, " {}", "failed".red().bold())
    }

    fn write_failure_with_message<W: Write>(
        &self,
        w: &mut W,
        message: &str,
    ) -> std::io::Result<()> {
        self.write_common_prefix(false, w)?;
        if message.is_empty() {
            write!(w, " {}", "failed".red().bold())
        } else {
            write!(w, " {}: {}", "failed".red().bold(), message)
        }
    }

    fn write_bench<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        self.write_common_prefix(true, w)?;
        write!(w, " {}", "ok".blue())
    }
}

#[derive(Debug, serde::Deserialize)]
#[expect(
    dead_code,
    reason = "Keep required wire fields even when the report does not display them."
)]
struct BenchSummary {
    name: Option<String>,
    min: f64,
    max: f64,
    mean: f64,
    median: f64,
    variance: f64,
    std_dev: f64,
    std_dev_pct: f64,
    median_abs_dev: f64,
    median_abs_dev_pct: f64,
    quartiles: (f64, f64, f64),
    iqr: f64,
    batch_size: usize,
    runs: usize,
}

#[derive(Debug, serde::Deserialize)]
struct BatchBenchSummaries {
    summaries: Vec<BenchSummary>,
}

fn auto_select_unit(us: f64) -> String {
    if us < 1.0 {
        format!("{:>6.2} ns", us * 1e3)
    } else if us < 1e3 {
        format!("{us:>6.2} µs")
    } else if us < 1e6 {
        format!("{:>6.2} ms", us / 1e3)
    } else if us < 6e7 {
        format!("{:>6.2} s", us / 1e6)
    } else {
        format!("{:>6.2} min", us / 6e7)
    }
}

fn render_batch_bench_summary(msg: &str) {
    assert!(msg.starts_with(BATCHBENCH));
    let msg = &msg[BATCHBENCH.len()..];
    let summary = serde_json_lenient::from_str::<BatchBenchSummaries>(msg)
        .unwrap_or_else(|e| panic!("failed to parse batch benchmark summary: {e}\n {msg}"));
    let max_name_len = summary
        .summaries
        .iter()
        .map(|s| s.name.as_ref().map_or(0, |n| n.len()))
        .max()
        .unwrap_or(0);
    if !summary.summaries.is_empty() {
        if max_name_len != 0 {
            print!("{:<width$} ", "name", width = max_name_len);
        }
        println!(
            "time ({} ± {})         range ({} … {}) ",
            "mean".bold().green(),
            "σ".green(),
            "min".blue(),
            "max".purple(),
        );
        let unknown_name = "".to_string();
        for s in summary.summaries {
            if max_name_len != 0 {
                print!(
                    "{:<width$} ",
                    s.name.as_ref().unwrap_or(&unknown_name),
                    width = max_name_len
                );
            }
            println!(
                " {} ± {}   {} … {}  in {} × {:>6} runs",
                auto_select_unit(s.mean).bold().green(),
                auto_select_unit(s.std_dev).green(),
                auto_select_unit(s.min).blue(),
                auto_select_unit(s.max).purple(),
                s.runs.to_string().bright_black(),
                s.batch_size.to_string().bright_black(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_select_unit_nanoseconds() {
        assert_eq!(auto_select_unit(0.5), "500.00 ns");
        assert_eq!(auto_select_unit(0.001), "  1.00 ns");
        assert_eq!(auto_select_unit(0.999), "999.00 ns");
    }

    #[test]
    fn auto_select_unit_microseconds() {
        assert_eq!(auto_select_unit(1.0), "  1.00 µs");
        assert_eq!(auto_select_unit(500.0), "500.00 µs");
    }

    #[test]
    fn auto_select_unit_milliseconds() {
        assert_eq!(auto_select_unit(1_000.0), "  1.00 ms");
        assert_eq!(auto_select_unit(500_000.0), "500.00 ms");
    }

    #[test]
    fn auto_select_unit_seconds() {
        assert_eq!(auto_select_unit(1_000_000.0), "  1.00 s");
    }

    #[test]
    fn auto_select_unit_minutes() {
        assert_eq!(auto_select_unit(60_000_001.0), "  1.00 min");
        assert_eq!(auto_select_unit(1_000_000_001.0), " 16.67 min");
    }
}
