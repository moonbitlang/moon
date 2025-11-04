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

pub mod filter_files;
pub mod prebuild_output;

use anyhow::Context;
use colored::*;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tracing::{debug, error, info, trace, warn};

/// The output of a watch run
pub struct WatchOutput {
    /// Whether the run was successful
    pub ok: bool,

    /// Additional paths to ignore in the next run
    pub additional_ignored_paths: Vec<PathBuf>,
}

/// Run a watcher that watches on `watch_dir`, and calls `run` when a file
/// changes. The watcher ignores changes in `original_target_dir`, and will
/// repopulate `target_dir` if it is deleted.
pub fn watching(
    run: impl Fn() -> anyhow::Result<WatchOutput>,
    watch_dir: &Path,
    source_dir: &Path,
    target_dir: &Path,
) -> anyhow::Result<i32> {
    // Initial run
    debug!(
        watch_dir = %watch_dir.display(),
        target_dir = %target_dir.display(),
        "Initial run before starting watcher"
    );
    let mut ignored_files = run_and_print(&run);

    // Setup watcher
    let (tx, rx) = std::sync::mpsc::channel();
    debug!("Creating file watcher with default config");
    let mut watcher = RecommendedWatcher::new(tx, Config::default())
        .context("Failed to create a directory watcher")?;

    // Setup Ctrl-C handler
    {
        // make sure the handler is only set once when --watch --target all
        static HANDLER_SET: AtomicBool = AtomicBool::new(false);

        if HANDLER_SET
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            ctrlc::set_handler(moonutil::common::dialoguer_ctrlc_handler)
                .expect("Error setting Ctrl-C handler");
            debug!("Ctrl-C handler registered for watch mode");
        }
    }

    // Start watching
    {
        // main thread
        info!(
            "Starting to watch directory '{}' recursively",
            watch_dir.display()
        );
        watcher
            .watch(watch_dir, RecursiveMode::Recursive)
            .with_context(|| format!("Failed to watch directory: '{}'", watch_dir.display()))?;

        // in watch mode, moon is a long-running process that should handle errors as much as possible rather than throwing them up and then exiting.
        const DEBOUNCE_TIME: Duration = Duration::from_millis(300);
        debug!("Watcher loop started (debounce = {:?})", DEBOUNCE_TIME);
        while let Ok(res) = rx.recv() {
            let Ok(evt) = res else {
                warn!(?res, "Watcher event channel returned an error");
                continue;
            };

            // Debounce events
            let mut evt_list = vec![evt];
            let start = std::time::Instant::now();
            while start.elapsed() < DEBOUNCE_TIME {
                if let Ok(Ok(evt)) = rx.recv_timeout(DEBOUNCE_TIME.saturating_sub(start.elapsed()))
                {
                    evt_list.push(evt);
                }
            }

            debug!("Debounced {} filesystem event(s)", evt_list.len());
            match check_rerun_trigger(target_dir, source_dir, &evt_list, &ignored_files) {
                Err(e) => {
                    error!(error = ?e, "Error while handling file change");
                    println!(
                        "{:?}\n{}",
                        e,
                        "Had errors, waiting for filesystem changes...".red().bold(),
                    );
                    continue;
                }
                Ok(false) => {
                    trace!("No rerun triggered after checking events");
                    continue;
                }
                Ok(true) => {
                    debug!("Rerun triggered; executing task");
                    ignored_files = run_and_print(&run);
                }
            }
        }
    }
    Ok(0)
}

/// Check if the event kind is relevant for a rebuild/rerun.
fn is_event_relevant(event: &notify::Event) -> bool {
    match event.kind {
        EventKind::Modify(notify::event::ModifyKind::Metadata(_)) => {
            trace!("Ignoring metadata-only modify event: {:?}", event);
            return false;
        }

        EventKind::Create(_) => (),
        EventKind::Modify(_) => (),
        EventKind::Remove(_) => (),
        _ => {
            info!(
                "Unknown file event: {:?}. Currently we skip them, but if this is a problem, please report to the developers.",
                event
            );
            return false;
        }
    };

    true
}

/// Determine if we should rerun based on the event. Returns true if we should rerun.
fn check_rerun_trigger(
    target_dir: &Path,
    project_root: &Path,
    event_lst: &[notify::Event],
    additional_ignored_paths: &HashSet<PathBuf>,
) -> anyhow::Result<bool> {
    debug!(
        "Evaluating {} filesystem event(s) for relevance",
        event_lst.len()
    );
    let relevant_events: Vec<&notify::Event> = event_lst
        .iter()
        .filter(|evt| is_event_relevant(evt))
        .collect();

    if relevant_events.is_empty() {
        trace!("No relevant changes detected; skipping run");
        return Ok(false);
    }

    // Check if any of the relevant events are in ignored dirs.
    // Note: `target/` and `.mooncakes/` are always ignored by default.
    let mut ignore_builder = filter_files::FileFilterBuilder::new(project_root);
    let mut trigger = false;
    'outer: for evt in &relevant_events {
        for path in &evt.paths {
            if ignore_builder.check_file(path) {
                trace!(
                    "Ignoring event for path '{}' due to ignore rules",
                    path.display()
                );
                continue;
            } else if additinal_ignored_paths.contains(path) {
                trace!(
                    "Ignoring event for path '{}' due to additional ignored paths",
                    path.display()
                );
                continue;
            } else {
                info!(
                    "Triggered by path '{}', event kind {:?}",
                    path.display(),
                    evt.kind
                );
                trigger = true;
                break 'outer;
            }
        }
    }
    info!("Triggered rerun");

    // prevent the case that the whole target_dir was deleted
    // FIXME: legacy code, might not need it
    if !target_dir.exists() {
        warn!(
            "Target directory '{}' missing; recreating it",
            target_dir.display()
        );
        std::fs::create_dir_all(target_dir).with_context(|| {
            format!(
                "Failed to create target directory: '{}'",
                target_dir.display()
            )
        })?;
    }

    Ok(trigger)
}

/// Clear the terminal and run the given function, printing success or error.
/// Returns additional paths to ignore in the next run.
fn run_and_print(run: impl FnOnce() -> anyhow::Result<WatchOutput>) -> HashSet<PathBuf> {
    debug!("Clearing terminal and running task");
    // print!("{esc}[2J{esc}[1;1H", esc = 27 as char);

    let result = run();
    match result {
        Ok(res) => {
            info!("Run completed without error, ok={}", res.ok);
            if res.ok {
                println!(
                    "{}",
                    "Success, waiting for filesystem changes...".green().bold()
                );
            } else {
                println!(
                    "{}",
                    "Had errors, waiting for filesystem changes...".red().bold(),
                );
            }
            HashSet::from_iter(res.additional_ignored_paths)
        }
        Err(e) => {
            error!(error = ?e, "Run failed with error");
            println!(
                "{:?}\n{}",
                e,
                "Had errors, waiting for filesystem changes...".red().bold(),
            );
            HashSet::new()
        }
    }
}
