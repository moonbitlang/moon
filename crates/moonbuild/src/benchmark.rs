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

use colored::Colorize;

pub const BATCHBENCH: &str = "@BATCH_BENCH ";

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct BenchSummary {
    pub name: Option<String>,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub median: f64,
    pub variance: f64,
    pub std_dev: f64,
    pub std_dev_pct: f64,
    pub median_abs_dev: f64,
    pub median_abs_dev_pct: f64,
    pub quartiles: (f64, f64, f64),
    pub iqr: f64,
    pub batch_size: usize,
    pub runs: usize,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct BatchBenchSummaries {
    pub summaries: Vec<BenchSummary>,
}

fn auto_select_unit(us: f64) -> String {
    if us < 1e3 {
        format!("{us:>6.2} µs")
    } else if us < 1e6 {
        format!("{:>6.2} ms", us / 1e3)
    } else if us < 1e9 {
        format!("{:>6.2} s", us / 1e6)
    } else {
        format!("{:>6.2} min", us / 6e10)
    }
}

pub fn render_batch_bench_summary(msg: &str) {
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
