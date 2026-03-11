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
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

struct PerfCounters {
    copy_tree_ops: AtomicU64,
    copy_tree_files: AtomicU64,
    copy_tree_ns: AtomicU64,
    process_ops: AtomicU64,
    process_ns: AtomicU64,
    moon_process_ops: AtomicU64,
    moon_process_ns: AtomicU64,
    other_process_ops: AtomicU64,
    other_process_ns: AtomicU64,
    normalize_output_ops: AtomicU64,
    normalize_output_ns: AtomicU64,
}

impl PerfCounters {
    fn new() -> Self {
        Self {
            copy_tree_ops: AtomicU64::new(0),
            copy_tree_files: AtomicU64::new(0),
            copy_tree_ns: AtomicU64::new(0),
            process_ops: AtomicU64::new(0),
            process_ns: AtomicU64::new(0),
            moon_process_ops: AtomicU64::new(0),
            moon_process_ns: AtomicU64::new(0),
            other_process_ops: AtomicU64::new(0),
            other_process_ns: AtomicU64::new(0),
            normalize_output_ops: AtomicU64::new(0),
            normalize_output_ns: AtomicU64::new(0),
        }
    }
}

#[derive(Clone, Copy)]
struct Snapshot {
    copy_tree_ops: u64,
    copy_tree_files: u64,
    copy_tree_ns: u64,
    process_ops: u64,
    process_ns: u64,
    moon_process_ops: u64,
    moon_process_ns: u64,
    other_process_ops: u64,
    other_process_ns: u64,
    normalize_output_ops: u64,
    normalize_output_ns: u64,
}

fn counters() -> &'static PerfCounters {
    static COUNTERS: OnceLock<PerfCounters> = OnceLock::new();
    COUNTERS.get_or_init(PerfCounters::new)
}

pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var_os("MOON_TEST_PERF")
            .map(|v| v != "0")
            .unwrap_or(false)
    })
}

fn summary_path() -> Option<&'static PathBuf> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| {
        if !enabled() {
            return None;
        }

        let path = std::env::var_os("MOON_TEST_PERF_FILE")
            .map(PathBuf::from)
            .map(add_pid_suffix)
            .unwrap_or_else(|| {
                std::env::temp_dir().join(format!("moon-test-perf-{}.txt", std::process::id()))
            });
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        eprintln!("[MOON_TEST_PERF] summary file: {}", path.display());
        Some(path)
    })
    .as_ref()
}

fn add_pid_suffix(path: PathBuf) -> PathBuf {
    let pid = std::process::id();
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return path.with_file_name(format!("moon-test-perf-{pid}.txt"));
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

fn duration_ns(duration: Duration) -> u64 {
    duration.as_nanos().min(u64::MAX as u128) as u64
}

fn snapshot() -> Snapshot {
    let counters = counters();
    Snapshot {
        copy_tree_ops: counters.copy_tree_ops.load(Ordering::Relaxed),
        copy_tree_files: counters.copy_tree_files.load(Ordering::Relaxed),
        copy_tree_ns: counters.copy_tree_ns.load(Ordering::Relaxed),
        process_ops: counters.process_ops.load(Ordering::Relaxed),
        process_ns: counters.process_ns.load(Ordering::Relaxed),
        moon_process_ops: counters.moon_process_ops.load(Ordering::Relaxed),
        moon_process_ns: counters.moon_process_ns.load(Ordering::Relaxed),
        other_process_ops: counters.other_process_ops.load(Ordering::Relaxed),
        other_process_ns: counters.other_process_ns.load(Ordering::Relaxed),
        normalize_output_ops: counters.normalize_output_ops.load(Ordering::Relaxed),
        normalize_output_ns: counters.normalize_output_ns.load(Ordering::Relaxed),
    }
}

fn avg_ms(total_ns: u64, ops: u64) -> f64 {
    if ops == 0 {
        0.0
    } else {
        total_ns as f64 / ops as f64 / 1_000_000.0
    }
}

fn total_ms(total_ns: u64) -> f64 {
    total_ns as f64 / 1_000_000.0
}

fn write_summary() {
    let Some(path) = summary_path() else {
        return;
    };
    let snapshot = snapshot();
    let summary = format!(
        concat!(
            "process_id: {}\n",
            "executable: {}\n",
            "copy_tree_ops: {}\n",
            "copy_tree_files: {}\n",
            "copy_tree_total_ms: {:.3}\n",
            "copy_tree_avg_ms: {:.3}\n",
            "process_ops: {}\n",
            "process_total_ms: {:.3}\n",
            "process_avg_ms: {:.3}\n",
            "moon_process_ops: {}\n",
            "moon_process_total_ms: {:.3}\n",
            "moon_process_avg_ms: {:.3}\n",
            "other_process_ops: {}\n",
            "other_process_total_ms: {:.3}\n",
            "other_process_avg_ms: {:.3}\n",
            "normalize_output_ops: {}\n",
            "normalize_output_total_ms: {:.3}\n",
            "normalize_output_avg_ms: {:.3}\n"
        ),
        std::process::id(),
        std::env::args()
            .next()
            .unwrap_or_else(|| "<unknown>".to_string()),
        snapshot.copy_tree_ops,
        snapshot.copy_tree_files,
        total_ms(snapshot.copy_tree_ns),
        avg_ms(snapshot.copy_tree_ns, snapshot.copy_tree_ops),
        snapshot.process_ops,
        total_ms(snapshot.process_ns),
        avg_ms(snapshot.process_ns, snapshot.process_ops),
        snapshot.moon_process_ops,
        total_ms(snapshot.moon_process_ns),
        avg_ms(snapshot.moon_process_ns, snapshot.moon_process_ops),
        snapshot.other_process_ops,
        total_ms(snapshot.other_process_ns),
        avg_ms(snapshot.other_process_ns, snapshot.other_process_ops),
        snapshot.normalize_output_ops,
        total_ms(snapshot.normalize_output_ns),
        avg_ms(snapshot.normalize_output_ns, snapshot.normalize_output_ops),
    );
    let _ = std::fs::write(path, summary);
}

pub fn record_copy_tree(duration: Duration, files: u64) {
    if !enabled() {
        return;
    }

    let counters = counters();
    counters.copy_tree_ops.fetch_add(1, Ordering::Relaxed);
    counters.copy_tree_files.fetch_add(files, Ordering::Relaxed);
    counters
        .copy_tree_ns
        .fetch_add(duration_ns(duration), Ordering::Relaxed);
    write_summary();
}

fn record_process(duration: Duration, is_moon: bool) {
    if !enabled() {
        return;
    }

    let counters = counters();
    counters.process_ops.fetch_add(1, Ordering::Relaxed);
    counters
        .process_ns
        .fetch_add(duration_ns(duration), Ordering::Relaxed);
    if is_moon {
        counters.moon_process_ops.fetch_add(1, Ordering::Relaxed);
        counters
            .moon_process_ns
            .fetch_add(duration_ns(duration), Ordering::Relaxed);
    } else {
        counters.other_process_ops.fetch_add(1, Ordering::Relaxed);
        counters
            .other_process_ns
            .fetch_add(duration_ns(duration), Ordering::Relaxed);
    }
    write_summary();
}

pub fn record_moon_process(duration: Duration) {
    record_process(duration, true);
}

pub fn record_other_process(duration: Duration) {
    record_process(duration, false);
}

pub fn record_normalize_output(duration: Duration) {
    if !enabled() {
        return;
    }

    let counters = counters();
    counters
        .normalize_output_ops
        .fetch_add(1, Ordering::Relaxed);
    counters
        .normalize_output_ns
        .fetch_add(duration_ns(duration), Ordering::Relaxed);
    write_summary();
}

pub fn measure_moon_process<T>(f: impl FnOnce() -> T) -> T {
    if !enabled() {
        return f();
    }

    let start = Instant::now();
    let result = f();
    record_moon_process(start.elapsed());
    result
}
