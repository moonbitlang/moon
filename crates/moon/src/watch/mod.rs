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

use anyhow::Context;
use colored::*;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tracing::info;

/// Run a watcher that watches on `watch_dir`, and calls `run` when a file
/// changes. The watcher ignores changes in `original_target_dir`, and will
/// repopulate `target_dir` if it is deleted.
pub fn watching(
    run: impl Fn() -> anyhow::Result<i32>,
    watch_dir: &Path,
    target_dir: &Path,
    original_target_dir: &Path,
) -> anyhow::Result<i32> {
    // Initial run
    run_and_print(&run);

    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = RecommendedWatcher::new(tx, Config::default())
        .context("Failed to create a directory watcher")?;

    {
        // make sure the handler is only set once when --watch --target all
        static HANDLER_SET: AtomicBool = AtomicBool::new(false);

        if HANDLER_SET
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            ctrlc::set_handler(moonutil::common::dialoguer_ctrlc_handler)
                .expect("Error setting Ctrl-C handler");
        }
    }

    {
        // main thread
        watcher
            .watch(watch_dir, RecursiveMode::Recursive)
            .with_context(|| format!("Failed to watch directory: '{}'", watch_dir.display()))?;

        // in watch mode, moon is a long-running process that should handle errors as much as possible rather than throwing them up and then exiting.
        const DEBOUNCE_TIME: Duration = Duration::from_millis(300);
        while let Ok(res) = rx.recv() {
            let Ok(evt) = res else {
                println!("failed: {res:?}");
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

            if let Err(e) = handle_file_change(&run, target_dir, original_target_dir, &evt_list) {
                println!(
                    "{:?}\n{}",
                    e,
                    "Had errors, waiting for filesystem changes...".red().bold(),
                );
            }
        }
    }
    Ok(0)
}

fn is_event_relevant(event: &notify::Event, original_target_dir: &Path) -> bool {
    match event.kind {
        EventKind::Modify(notify::event::ModifyKind::Metadata(_)) => return false,

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

    // Skip if the change happens in the target dir, which is related to the
    // build output (of any kind) and should not trigger a rebuild.
    if event
        .paths
        .iter()
        .all(|p| p.starts_with(original_target_dir))
    {
        return false;
    }
    true
}

/// Determine if we should rerun based on the event, and run if so.
fn handle_file_change(
    run: impl FnOnce() -> anyhow::Result<i32>,
    target_dir: &Path,
    original_target_dir: &Path,
    event_lst: &[notify::Event],
) -> anyhow::Result<()> {
    let is_relevant = event_lst
        .iter()
        .any(|evt| is_event_relevant(evt, original_target_dir));

    if !is_relevant {
        return Ok(());
    }

    // prevent the case that the whole target_dir was deleted
    // FIXME: legacy code, might not need it
    if !target_dir.exists() {
        std::fs::create_dir_all(target_dir).with_context(|| {
            format!(
                "Failed to create target directory: '{}'",
                target_dir.display()
            )
        })?;
    }

    run_and_print(run);
    Ok(())
}

/// Clear the terminal and run the given function, printing success or error
fn run_and_print(run: impl FnOnce() -> anyhow::Result<i32>) {
    print!("{esc}[2J{esc}[1;1H", esc = 27 as char);
    let result = run();
    match result {
        Ok(0) => {
            println!(
                "{}",
                "Success, waiting for filesystem changes...".green().bold()
            );
        }
        Err(e) => {
            println!(
                "{:?}\n{}",
                e,
                "Had errors, waiting for filesystem changes...".red().bold(),
            );
        }
        _ => {
            println!(
                "{}",
                "Had errors, waiting for filesystem changes...".red().bold(),
            );
        }
    }
}
