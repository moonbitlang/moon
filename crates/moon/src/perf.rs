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

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub struct ChildPerf {
    path: Option<PathBuf>,
    start: Instant,
}

impl ChildPerf {
    pub fn start() -> Self {
        let path = std::env::var_os("MOON_TEST_CHILD_PERF_FILE")
            .filter(|_| std::env::var_os("MOON_TEST_CHILD_PERF_DEPTH").is_none())
            .map(PathBuf::from)
            .map(add_pid_suffix);

        if path.is_some() {
            // Set a depth marker early so descendant `moon` processes do not
            // emit duplicate reports for the same top-level test harness call.
            unsafe {
                std::env::set_var("MOON_TEST_CHILD_PERF_DEPTH", "1");
            }
        }

        if let Some(path) = &path {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
        }

        Self {
            path,
            start: Instant::now(),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    pub fn write_summary(
        &self,
        command_name: &str,
        setup_duration: Duration,
        command_duration: Duration,
    ) {
        let Some(path) = &self.path else {
            return;
        };

        let main_duration = self.start.elapsed();
        let post_command_duration =
            main_duration.saturating_sub(setup_duration.saturating_add(command_duration));
        let summary = format!(
            concat!(
                "process_id: {}\n",
                "executable: {}\n",
                "command_name: {}\n",
                "main_total_ms: {:.3}\n",
                "setup_total_ms: {:.3}\n",
                "command_total_ms: {:.3}\n",
                "post_command_total_ms: {:.3}\n"
            ),
            std::process::id(),
            std::env::args()
                .next()
                .unwrap_or_else(|| "<unknown>".to_string()),
            command_name,
            total_ms(main_duration),
            total_ms(setup_duration),
            total_ms(command_duration),
            total_ms(post_command_duration),
        );
        let _ = std::fs::write(path, summary);
    }
}

fn add_pid_suffix(path: PathBuf) -> PathBuf {
    let pid = std::process::id();
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return path.with_file_name(format!("moon-child-perf-{pid}.txt"));
    };
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(file_name);
    let extension = path.extension().and_then(|ext| ext.to_str());
    let suffixed = match extension {
        Some(ext) => format!("{stem}-{pid}.{ext}"),
        None => format!("{stem}-{pid}"),
    };
    path.with_file_name(suffixed)
}

fn total_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}
