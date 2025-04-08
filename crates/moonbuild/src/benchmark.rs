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

pub const BENCH: &str = "@BENCH ";

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct BenchSummary {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub median: f64,
    pub var: f64,
    pub std_dev: f64,
    pub std_dev_pct: f64,
    pub median_abs_dev: f64,
    pub median_abs_dev_pct: f64,
    pub quartiles: (f64, f64, f64),
    pub iqr: f64,
    pub batch_size: usize,
    pub runs: usize,
}

fn auto_select_unit(us: f64) -> String {
    if us < 1e3 {
        format!("{:.2} µs", us)
    } else if us < 1e6 {
        format!("{:.2} ms", us / 1e3)
    } else if us < 1e9 {
        format!("{:.2} s", us / 1e6)
    } else {
        format!("{:.2} min", us / 6e10)
    }
}

pub fn render_bench_summary(msg: &str) {
    assert!(msg.starts_with(BENCH));
    let msg = &msg[BENCH.len()..];
    let summary = serde_json_lenient::from_str::<BenchSummary>(msg)
        .unwrap_or_else(|_| panic!("failed to parse benchmark summary: {}", msg));
    println!(
        "time ({} ± {}) range ({} … {}) in {} × {} runs",
        auto_select_unit(summary.mean).bold().green(),
        auto_select_unit(summary.std_dev).green(),
        auto_select_unit(summary.min).blue(),
        auto_select_unit(summary.max).purple(),
        summary.runs.to_string().bright_black(),
        summary.batch_size.to_string().bright_black(),
    )
}
