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

pub(crate) mod filter_files;
pub(crate) mod prebuild_output;

use anyhow::Context;
use colored::*;
use moonutil::constants::is_watch_relevant_project_file;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tracing::{debug, error, info, trace, warn};

/// The output of a watch run
pub(crate) struct WatchOutput {
    /// Whether the run was successful
    pub ok: bool,

    /// Additional paths to ignore in the next run
    pub additional_ignored_paths: Vec<PathBuf>,

    /// Additional paths to watch in the next run
    pub additional_watched_paths: Vec<PathBuf>,
}

#[derive(Default)]
struct AdditionalWatchPaths {
    ignored_paths: HashSet<PathBuf>,
    watched_paths: HashSet<PathBuf>,
}

/// Watch the source tree and call `run` when a relevant input changes.
pub(crate) fn watching(
    run: impl Fn() -> anyhow::Result<WatchOutput>,
    watch_root: &Path,
    ignored_subtree: &Path,
) -> anyhow::Result<i32> {
    // Initial run
    debug!(
        watch_root = %watch_root.display(),
        "Initial run before starting watcher"
    );
    let mut additional_paths = run_and_print(&run);

    // Setup watcher
    let (tx, rx) = std::sync::mpsc::channel();
    debug!(
        backend = ?RecommendedWatcher::kind(),
        "Creating file watcher with default config"
    );
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
            ctrlc::set_handler(moonutil::cli_support::dialoguer_ctrlc_handler)
                .expect("Error setting Ctrl-C handler");
            debug!("Ctrl-C handler registered for watch mode");
        }
    }

    // Start watching
    {
        // main thread
        info!(
            "Starting to watch directory '{}' recursively",
            watch_root.display()
        );
        watcher
            .watch(watch_root, RecursiveMode::Recursive)
            .with_context(|| format!("Failed to watch directory: '{}'", watch_root.display()))?;
        info!(
            backend = ?RecommendedWatcher::kind(),
            "Using file watcher backend"
        );

        // in watch mode, moon is a long-running process that should handle errors as much as possible rather than throwing them up and then exiting.
        const DEBOUNCE_TIME: Duration = Duration::from_millis(300);
        debug!("Watcher loop started (debounce = {:?})", DEBOUNCE_TIME);
        while let Ok(res) = rx.recv() {
            let evt = match res {
                Ok(evt) => {
                    log_watch_event(&evt);
                    evt
                }
                Err(err) => {
                    warn!(error = ?err, "Watcher event channel returned an error");
                    continue;
                }
            };

            // Debounce events
            let mut evt_list = vec![evt];
            let start = std::time::Instant::now();
            while start.elapsed() < DEBOUNCE_TIME {
                match rx.recv_timeout(DEBOUNCE_TIME.saturating_sub(start.elapsed())) {
                    Ok(Ok(evt)) => {
                        log_watch_event(&evt);
                        evt_list.push(evt);
                    }
                    Ok(Err(err)) => {
                        warn!(error = ?err, "Watcher event channel returned an error");
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }

            debug!("Debounced {} filesystem event(s)", evt_list.len());
            if !check_rerun_trigger(watch_root, ignored_subtree, &evt_list, &additional_paths) {
                trace!("No rerun triggered after checking events");
                continue;
            }

            debug!("Rerun triggered; executing task");
            additional_paths = run_and_print(&run);
        }
    }
    Ok(0)
}

fn log_watch_event(event: &notify::Event) {
    debug!(event = ?event, "Received filesystem event");
}

/// Check if the event kind is relevant for a rebuild/rerun.
fn is_event_relevant(event: &notify::Event) -> bool {
    match event.kind {
        EventKind::Modify(notify::event::ModifyKind::Metadata(_)) => {
            trace!("Ignoring metadata-only modify event: {:?}", event);
            return false;
        }
        EventKind::Access(_) => return false,

        EventKind::Create(_) => (),
        EventKind::Modify(_) => (),
        EventKind::Remove(_) => (),
        _ => {
            debug!(
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
    repo_root: &Path,
    ignored_subtree: &Path,
    event_lst: &[notify::Event],
    additional_paths: &AdditionalWatchPaths,
) -> bool {
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
        return false;
    }

    let trigger = check_paths(
        repo_root,
        ignored_subtree,
        additional_paths,
        &relevant_events,
    );
    debug!("Have we triggered a rebuild?: {}", trigger);
    trigger
}

// Check the paths in the events against the ignore rules.
fn check_paths(
    repo_root: &Path,
    ignored_subtree: &Path,
    additional_paths: &AdditionalWatchPaths,
    relevant_events: &[&notify::Event],
) -> bool {
    // Check if any of the relevant events are in ignored dirs.
    // Note: `_build/` and `.mooncakes/` are always ignored by default.
    let mut ignore_builder = filter_files::FileFilterBuilder::new(repo_root);
    // Only a proper descendant can be filtered as part of the watched tree.
    // An equal or ancestor directory would also suppress source events.
    let filters_output_subtree =
        ignored_subtree != repo_root && ignored_subtree.starts_with(repo_root);

    for evt in relevant_events {
        for path in &evt.paths {
            if path_matches(&additional_paths.ignored_paths, path) {
                trace!(
                    "Ignoring event for path '{}' due to additional ignored paths",
                    path.display()
                );
                continue;
            }

            let explicitly_watched = path_matches(&additional_paths.watched_paths, path);

            if !explicitly_watched && filters_output_subtree && path.starts_with(ignored_subtree) {
                trace!(
                    "Ignoring event for path '{}' because it is in the output directory",
                    path.display()
                );
                continue;
            }

            // Filter to source/config files that can affect RR planning/builds.
            // Note: A file removal will render `path.is_file()` false, but we
            // should still trigger a rerun in that case.
            if path.is_file()
                && !evt.kind.is_remove()
                && !explicitly_watched
                && let Some(fname) = path.file_name()
            {
                let lossy_fname = fname.to_string_lossy();
                if !is_watch_relevant_project_file(&lossy_fname) {
                    trace!(
                        "Ignoring event for path '{}' due to filename filter",
                        path.display()
                    );
                    continue;
                }
            }
            if !explicitly_watched && ignore_builder.check_file(path) {
                trace!(
                    "Ignoring event for path '{}' due to ignore rules",
                    path.display()
                );
                continue;
            } else {
                info!(
                    "Triggered by path '{}', event kind {:?}",
                    path.display(),
                    evt.kind
                );
                return true;
            }
        }
    }

    false
}

fn path_matches(paths: &HashSet<PathBuf>, path: &Path) -> bool {
    if paths.contains(path) {
        return true;
    }

    let normalized_path = normalize_watch_path(path);
    if paths.contains(&normalized_path) {
        return true;
    }

    paths.iter().any(|candidate| {
        candidate != path
            && candidate != &normalized_path
            && normalize_watch_path(candidate) == normalized_path
    })
}

fn normalize_watch_path(path: &Path) -> PathBuf {
    if let Ok(canonical) = dunce::canonicalize(path) {
        return canonical;
    }

    if let (Some(parent), Some(filename)) = (path.parent(), path.file_name())
        && let Ok(canonical_parent) = dunce::canonicalize(parent)
    {
        return canonical_parent.join(filename);
    }

    path.to_path_buf()
}

/// Clear the terminal and run the given function, printing success or error.
/// Returns additional paths to watch or ignore in the next run.
fn run_and_print(run: impl FnOnce() -> anyhow::Result<WatchOutput>) -> AdditionalWatchPaths {
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
            AdditionalWatchPaths {
                ignored_paths: HashSet::from_iter(
                    res.additional_ignored_paths
                        .into_iter()
                        .map(|path| normalize_watch_path(&path)),
                ),
                watched_paths: HashSet::from_iter(
                    res.additional_watched_paths
                        .into_iter()
                        .map(|path| normalize_watch_path(&path)),
                ),
            }
        }
        Err(e) => {
            error!(error = ?e, "Run failed with error");
            println!(
                "{:?}\n{}",
                e,
                "Had errors, waiting for filesystem changes...".red().bold(),
            );
            AdditionalWatchPaths::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use moonutil::constants::MOON_WORK;
    use notify::event::{CreateKind, Event, EventKind};

    fn build_event(path: &Path) -> notify::Event {
        Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![path.to_path_buf()],
            attrs: Default::default(),
        }
    }

    fn check_events(
        root: &Path,
        events: &[notify::Event],
        additional_paths: &AdditionalWatchPaths,
    ) -> bool {
        let ignored_subtree = root.join("_build");
        check_rerun_trigger(root, &ignored_subtree, events, additional_paths)
    }

    #[test]
    fn rerun_not_triggered_when_no_relevant_events() {
        let temp_dir = tempfile::tempdir().unwrap();
        let result = check_events(temp_dir.path(), &[], &AdditionalWatchPaths::default());

        assert!(!result);
    }

    #[test]
    fn rerun_ignored_for_ignored_paths() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
        let file = root.join("ignored.txt");
        fs::write(&file, "data").unwrap();

        let event = build_event(&file);
        let result = check_events(root, &[event], &AdditionalWatchPaths::default());

        assert!(!result);
    }

    #[test]
    fn rerun_triggered_for_relevant_file() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        let file = root.join("src/main.mbt");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "stuff").unwrap();

        let event = build_event(&file);
        let result = check_events(root, &[event], &AdditionalWatchPaths::default());

        assert!(result);
    }

    #[test]
    fn rerun_triggered_when_output_directory_is_not_a_strict_descendant() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp_dir.path()).unwrap();
        let watch_root = root.join("project");
        let disjoint_output = root.join("target");

        let file = watch_root.join("src/main.mbt");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::create_dir_all(&disjoint_output).unwrap();
        fs::write(&file, "stuff").unwrap();

        let event = build_event(&file);
        let additional_paths = AdditionalWatchPaths::default();

        for output_dir in [
            root.as_path(),
            watch_root.as_path(),
            disjoint_output.as_path(),
        ] {
            let result = check_rerun_trigger(
                &watch_root,
                output_dir,
                std::slice::from_ref(&event),
                &additional_paths,
            );

            assert!(
                result,
                "output directory '{}' suppressed a source event",
                output_dir.display()
            );
        }
    }

    #[test]
    fn rerun_triggered_for_workspace_manifest() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        let file = root.join(MOON_WORK);
        fs::write(&file, "members = [\"./app\"]").unwrap();

        let event = build_event(&file);
        let result = check_events(root, &[event], &AdditionalWatchPaths::default());

        assert!(result);
    }

    #[test]
    fn rerun_triggered_for_moon_pkg_dsl() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        let file = root.join("main/moon.pkg");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "is-main = true").unwrap();

        let event = build_event(&file);
        let result = check_events(root, &[event], &AdditionalWatchPaths::default());

        assert!(result);
    }

    #[test]
    fn rerun_triggered_for_moonlex_input() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        let file = root.join("src/main/lexer.mbl");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "rule token = parse").unwrap();

        let event = build_event(&file);
        let result = check_events(root, &[event], &AdditionalWatchPaths::default());

        assert!(result);
    }

    #[test]
    fn rerun_triggered_for_moonyacc_input() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        let file = root.join("src/main/parser.mby");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "%%").unwrap();

        let event = build_event(&file);
        let result = check_events(root, &[event], &AdditionalWatchPaths::default());

        assert!(result);
    }

    #[test]
    fn rerun_triggered_for_explicitly_watched_prebuild_input() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        let file = root.join("src/lib/input.txt");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(root.join(".gitignore"), "src/lib/input.txt\n").unwrap();
        fs::write(&file, "data").unwrap();

        let event = build_event(&file);
        let result = check_events(
            root,
            &[event],
            &AdditionalWatchPaths {
                watched_paths: HashSet::from_iter([dunce::canonicalize(file).unwrap()]),
                ..AdditionalWatchPaths::default()
            },
        );

        assert!(result);
    }

    #[test]
    fn rerun_triggered_for_explicitly_watched_prebuild_input_in_output_directory() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        let file = root.join("_build/input.txt");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "data").unwrap();

        let event = build_event(&file);
        let result = check_events(
            root,
            &[event],
            &AdditionalWatchPaths {
                watched_paths: HashSet::from_iter([dunce::canonicalize(file).unwrap()]),
                ..AdditionalWatchPaths::default()
            },
        );

        assert!(result);
    }

    #[test]
    fn rerun_triggered_for_explicitly_watched_prebuild_input_with_dot_segments() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        let file = root.join("src/lib/input.txt");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "data").unwrap();

        let event = build_event(&file);
        let result = check_events(
            root,
            &[event],
            &AdditionalWatchPaths {
                watched_paths: HashSet::from_iter([root.join("./src/lib/input.txt")]),
                ..AdditionalWatchPaths::default()
            },
        );

        assert!(result);
    }

    #[test]
    fn rerun_ignored_for_explicitly_ignored_prebuild_output() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        let file = root.join("src/lib/generated.mbt");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "fn generated() {}").unwrap();

        let event = build_event(&file);
        let result = check_events(
            root,
            &[event],
            &AdditionalWatchPaths {
                ignored_paths: HashSet::from_iter([dunce::canonicalize(file).unwrap()]),
                ..AdditionalWatchPaths::default()
            },
        );

        assert!(!result);
    }

    #[test]
    fn rerun_ignored_for_explicitly_ignored_prebuild_output_with_dot_segments() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        let file = root.join("src/lib/generated.mbt");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "fn generated() {}").unwrap();

        let event = build_event(&file);
        let result = check_events(
            root,
            &[event],
            &AdditionalWatchPaths {
                ignored_paths: HashSet::from_iter([root.join("./src/lib/generated.mbt")]),
                ..AdditionalWatchPaths::default()
            },
        );

        assert!(!result);
    }

    #[test]
    fn rerun_ignored_when_configured_output_directory_is_removed() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp_dir.path()).unwrap();
        let output_dir = root.join("target");
        fs::create_dir_all(&output_dir).unwrap();
        let ignored_subtree = normalize_watch_path(&output_dir);
        fs::remove_dir(&output_dir).unwrap();

        let event = notify::Event {
            kind: EventKind::Remove(notify::event::RemoveKind::Folder),
            paths: vec![ignored_subtree.clone()],
            attrs: Default::default(),
        };
        let result = check_rerun_trigger(
            &root,
            &ignored_subtree,
            &[event],
            &AdditionalWatchPaths::default(),
        );

        assert!(!result);
    }
}
