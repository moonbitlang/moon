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
use moonutil::common::is_watch_relevant_project_file;
#[cfg(target_os = "macos")]
use notify::PollWatcher;
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

type WatchEventSender = std::sync::mpsc::Sender<notify::Result<notify::Event>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WatchBackend {
    #[cfg(target_os = "macos")]
    Kqueue,
    #[cfg(target_os = "macos")]
    Poll,
    #[cfg(not(target_os = "macos"))]
    Recommended,
}

impl WatchBackend {
    fn label(self) -> &'static str {
        match self {
            #[cfg(target_os = "macos")]
            WatchBackend::Kqueue => "kqueue",
            #[cfg(target_os = "macos")]
            WatchBackend::Poll => "poll",
            #[cfg(not(target_os = "macos"))]
            WatchBackend::Recommended => "recommended",
        }
    }
}

struct ActiveWatcher {
    backend: WatchBackend,
    _watcher: Box<dyn Watcher>,
}

#[cfg(target_os = "macos")]
const POLL_WATCHER_INTERVAL: Duration = Duration::from_secs(1);

/// Run a watcher that watches on `watch_dir`, and calls `run` when a file
/// changes. The watcher ignores changes in `original_target_dir`, and will
/// repopulate `target_dir` if it is deleted.
pub(crate) fn watching(
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
    let mut additional_paths = run_and_print(&run);

    // Setup watcher
    let (tx, rx) = std::sync::mpsc::channel();

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
        let mut watcher = create_active_watcher(tx.clone(), watch_dir)?;

        // in watch mode, moon is a long-running process that should handle errors as much as possible rather than throwing them up and then exiting.
        const DEBOUNCE_TIME: Duration = Duration::from_millis(300);
        debug!("Watcher loop started (debounce = {:?})", DEBOUNCE_TIME);
        while let Ok(res) = rx.recv() {
            let Ok(evt) = res else {
                handle_watcher_error(&mut watcher, tx.clone(), watch_dir, res.unwrap_err());
                continue;
            };

            // Debounce events
            let mut evt_list = vec![evt];
            let start = std::time::Instant::now();
            while start.elapsed() < DEBOUNCE_TIME {
                match rx.recv_timeout(DEBOUNCE_TIME.saturating_sub(start.elapsed())) {
                    Ok(Ok(evt)) => evt_list.push(evt),
                    Ok(Err(err)) => handle_watcher_error(&mut watcher, tx.clone(), watch_dir, err),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }

            debug!("Debounced {} filesystem event(s)", evt_list.len());
            match check_rerun_trigger(target_dir, source_dir, &evt_list, &additional_paths) {
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
                    additional_paths = run_and_print(&run);
                }
            }
        }
    }
    Ok(0)
}

fn create_active_watcher(
    event_tx: WatchEventSender,
    watch_dir: &Path,
) -> anyhow::Result<ActiveWatcher> {
    #[cfg(target_os = "macos")]
    {
        let kqueue_tx = event_tx.clone();
        let poll_tx = event_tx.clone();
        let (watcher, backend_label) = try_with_fallback(
            WatchBackend::Kqueue.label(),
            || create_kqueue_watcher(kqueue_tx, watch_dir),
            WatchBackend::Poll.label(),
            || create_poll_watcher(poll_tx, watch_dir),
        )?;
        info!(backend = backend_label, "Using file watcher backend");
        return Ok(watcher);
    }

    #[cfg(not(target_os = "macos"))]
    {
        let watcher = create_recommended_watcher(event_tx, watch_dir)?;
        info!(
            backend = watcher.backend.label(),
            "Using file watcher backend"
        );
        Ok(watcher)
    }
}

fn start_watcher<W>(
    backend: WatchBackend,
    event_tx: WatchEventSender,
    config: Config,
    watch_dir: &Path,
) -> anyhow::Result<ActiveWatcher>
where
    W: Watcher + 'static,
{
    debug!(backend = backend.label(), "Creating file watcher");
    let mut watcher = W::new(event_tx, config)
        .with_context(|| format!("Failed to create {} file watcher", backend.label()))?;
    watcher
        .watch(watch_dir, RecursiveMode::Recursive)
        .with_context(|| {
            format!(
                "Failed to watch directory '{}' with {} watcher",
                watch_dir.display(),
                backend.label()
            )
        })?;

    Ok(ActiveWatcher {
        backend,
        _watcher: Box::new(watcher),
    })
}

#[cfg(not(target_os = "macos"))]
fn create_recommended_watcher(
    event_tx: WatchEventSender,
    watch_dir: &Path,
) -> anyhow::Result<ActiveWatcher> {
    start_watcher::<RecommendedWatcher>(
        WatchBackend::Recommended,
        event_tx,
        Config::default(),
        watch_dir,
    )
}

#[cfg(target_os = "macos")]
fn create_kqueue_watcher(
    event_tx: WatchEventSender,
    watch_dir: &Path,
) -> anyhow::Result<ActiveWatcher> {
    start_watcher::<RecommendedWatcher>(
        WatchBackend::Kqueue,
        event_tx,
        Config::default(),
        watch_dir,
    )
}

#[cfg(target_os = "macos")]
fn create_poll_watcher(
    event_tx: WatchEventSender,
    watch_dir: &Path,
) -> anyhow::Result<ActiveWatcher> {
    start_watcher::<PollWatcher>(
        WatchBackend::Poll,
        event_tx,
        Config::default().with_poll_interval(POLL_WATCHER_INTERVAL),
        watch_dir,
    )
}

fn try_with_fallback<T>(
    primary_label: &'static str,
    primary: impl FnOnce() -> anyhow::Result<T>,
    fallback_label: &'static str,
    fallback: impl FnOnce() -> anyhow::Result<T>,
) -> anyhow::Result<(T, &'static str)> {
    match primary() {
        Ok(value) => Ok((value, primary_label)),
        Err(primary_error) => {
            warn!(
                backend = primary_label,
                fallback_backend = fallback_label,
                error = ?primary_error,
                "Primary file watcher backend failed; trying fallback",
            );
            match fallback() {
                Ok(value) => Ok((value, fallback_label)),
                Err(fallback_error) => Err(anyhow::anyhow!(
                    "Failed to initialize {} watcher: {:#}\nFallback {} watcher also failed: {:#}",
                    primary_label,
                    primary_error,
                    fallback_label,
                    fallback_error
                )),
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn handle_watcher_error(
    watcher: &mut ActiveWatcher,
    event_tx: WatchEventSender,
    watch_dir: &Path,
    error: notify::Error,
) {
    if watcher.backend != WatchBackend::Kqueue {
        warn!(
            backend = watcher.backend.label(),
            error = ?error,
            "Watcher event channel returned an error",
        );
        return;
    }

    warn!(
        backend = watcher.backend.label(),
        fallback_backend = WatchBackend::Poll.label(),
        error = ?error,
        "Watcher backend reported an error; switching to polling fallback",
    );

    match create_poll_watcher(event_tx, watch_dir) {
        Ok(poll_watcher) => {
            *watcher = poll_watcher;
            warn!(
                backend = watcher.backend.label(),
                poll_interval_ms = POLL_WATCHER_INTERVAL.as_millis() as u64,
                "Polling fallback enabled after watcher error",
            );
        }
        Err(fallback_error) => {
            warn!(
                error = ?fallback_error,
                "Polling fallback failed after watcher error",
            );
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn handle_watcher_error(
    watcher: &mut ActiveWatcher,
    _event_tx: WatchEventSender,
    _watch_dir: &Path,
    error: notify::Error,
) {
    warn!(
        backend = watcher.backend.label(),
        error = ?error,
        "Watcher event channel returned an error",
    );
}

/// Check if the event kind is relevant for a rebuild/rerun.
fn is_event_relevant(event: &notify::Event) -> bool {
    match event.kind {
        EventKind::Modify(notify::event::ModifyKind::Metadata(
            notify::event::MetadataKind::WriteTime,
        )) if event.paths.iter().all(|path| path.is_file()) => (),
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
    target_dir: &Path,
    repo_root: &Path,
    event_lst: &[notify::Event],
    additional_paths: &AdditionalWatchPaths,
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

    let trigger = check_paths(repo_root, additional_paths, &relevant_events);
    debug!("Have we triggered a rebuild?: {}", trigger);

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

// Check the paths in the events against the ignore rules.
fn check_paths(
    repo_root: &Path,
    additional_paths: &AdditionalWatchPaths,
    relevant_events: &[&notify::Event],
) -> bool {
    // Check if any of the relevant events are in ignored dirs.
    // Note: `_build/` and `.mooncakes/` are always ignored by default.
    let mut ignore_builder = filter_files::FileFilterBuilder::new(repo_root);

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

    use anyhow::anyhow;
    use moonutil::common::{BUILD_DIR, MOON_WORK, MOON_WORK_JSON};
    use notify::event::{CreateKind, Event, EventKind, MetadataKind, ModifyKind};

    fn build_event(path: &Path) -> notify::Event {
        Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![path.to_path_buf()],
            attrs: Default::default(),
        }
    }

    fn metadata_write_time_event(path: &Path) -> notify::Event {
        Event {
            kind: EventKind::Modify(ModifyKind::Metadata(MetadataKind::WriteTime)),
            paths: vec![path.to_path_buf()],
            attrs: Default::default(),
        }
    }

    #[test]
    fn rerun_not_triggered_when_no_relevant_events() {
        let temp_dir = tempfile::tempdir().unwrap();
        let target_dir = temp_dir.path().join(BUILD_DIR);
        std::fs::create_dir_all(&target_dir).unwrap();

        let result = check_rerun_trigger(
            &target_dir,
            temp_dir.path(),
            &[],
            &AdditionalWatchPaths::default(),
        )
        .unwrap();

        assert!(!result);
    }

    #[test]
    fn rerun_ignored_for_ignored_paths() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        let target_dir = root.join(BUILD_DIR);
        std::fs::create_dir_all(&target_dir).unwrap();

        fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
        let file = root.join("ignored.txt");
        fs::write(&file, "data").unwrap();

        let event = build_event(&file);
        let result = check_rerun_trigger(
            &target_dir,
            root,
            &[event],
            &AdditionalWatchPaths::default(),
        )
        .unwrap();

        assert!(!result);
    }

    #[test]
    fn rerun_triggered_for_relevant_file() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        let target_dir = root.join(BUILD_DIR);
        std::fs::create_dir_all(&target_dir).unwrap();

        let file = root.join("src/main.mbt");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "stuff").unwrap();

        let event = build_event(&file);
        let result = check_rerun_trigger(
            &target_dir,
            root,
            &[event],
            &AdditionalWatchPaths::default(),
        )
        .unwrap();

        assert!(result);
    }

    #[test]
    fn rerun_triggered_for_workspace_manifest() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        let target_dir = root.join(BUILD_DIR);
        std::fs::create_dir_all(&target_dir).unwrap();

        let file = root.join(MOON_WORK);
        fs::write(&file, "members = [\"./app\"]").unwrap();

        let event = build_event(&file);
        let result = check_rerun_trigger(
            &target_dir,
            root,
            &[event],
            &AdditionalWatchPaths::default(),
        )
        .unwrap();

        assert!(result);
    }

    #[test]
    fn rerun_triggered_for_legacy_workspace_manifest() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        let target_dir = root.join(BUILD_DIR);
        std::fs::create_dir_all(&target_dir).unwrap();

        let file = root.join(MOON_WORK_JSON);
        fs::write(&file, "{ \"use\": [\"./app\"] }").unwrap();

        let event = build_event(&file);
        let result = check_rerun_trigger(
            &target_dir,
            root,
            &[event],
            &AdditionalWatchPaths::default(),
        )
        .unwrap();

        assert!(result);
    }

    #[test]
    fn rerun_triggered_for_moon_pkg_dsl() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        let target_dir = root.join(BUILD_DIR);
        std::fs::create_dir_all(&target_dir).unwrap();

        let file = root.join("main/moon.pkg");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "is-main = true").unwrap();

        let event = build_event(&file);
        let result = check_rerun_trigger(
            &target_dir,
            root,
            &[event],
            &AdditionalWatchPaths::default(),
        )
        .unwrap();

        assert!(result);
    }

    #[test]
    fn rerun_triggered_for_moonlex_input() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        let target_dir = root.join(BUILD_DIR);
        std::fs::create_dir_all(&target_dir).unwrap();

        let file = root.join("src/main/lexer.mbl");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "rule token = parse").unwrap();

        let event = build_event(&file);
        let result = check_rerun_trigger(
            &target_dir,
            root,
            &[event],
            &AdditionalWatchPaths::default(),
        )
        .unwrap();

        assert!(result);
    }

    #[test]
    fn rerun_triggered_for_moonyacc_input() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        let target_dir = root.join(BUILD_DIR);
        std::fs::create_dir_all(&target_dir).unwrap();

        let file = root.join("src/main/parser.mby");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "%%").unwrap();

        let event = build_event(&file);
        let result = check_rerun_trigger(
            &target_dir,
            root,
            &[event],
            &AdditionalWatchPaths::default(),
        )
        .unwrap();

        assert!(result);
    }

    #[test]
    fn rerun_triggered_for_explicitly_watched_prebuild_input() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        let target_dir = root.join(BUILD_DIR);
        std::fs::create_dir_all(&target_dir).unwrap();

        let file = root.join("src/lib/input.txt");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(root.join(".gitignore"), "src/lib/input.txt\n").unwrap();
        fs::write(&file, "data").unwrap();

        let event = build_event(&file);
        let result = check_rerun_trigger(
            &target_dir,
            root,
            &[event],
            &AdditionalWatchPaths {
                ignored_paths: HashSet::new(),
                watched_paths: HashSet::from_iter([dunce::canonicalize(file).unwrap()]),
            },
        )
        .unwrap();

        assert!(result);
    }

    #[test]
    fn rerun_triggered_for_explicitly_watched_prebuild_input_with_dot_segments() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        let target_dir = root.join(BUILD_DIR);
        std::fs::create_dir_all(&target_dir).unwrap();

        let file = root.join("src/lib/input.txt");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "data").unwrap();

        let event = build_event(&file);
        let result = check_rerun_trigger(
            &target_dir,
            root,
            &[event],
            &AdditionalWatchPaths {
                ignored_paths: HashSet::new(),
                watched_paths: HashSet::from_iter([root.join("./src/lib/input.txt")]),
            },
        )
        .unwrap();

        assert!(result);
    }

    #[test]
    fn rerun_ignored_for_explicitly_ignored_prebuild_output() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        let target_dir = root.join(BUILD_DIR);
        std::fs::create_dir_all(&target_dir).unwrap();

        let file = root.join("src/lib/generated.mbt");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "fn generated() {}").unwrap();

        let event = build_event(&file);
        let result = check_rerun_trigger(
            &target_dir,
            root,
            &[event],
            &AdditionalWatchPaths {
                ignored_paths: HashSet::from_iter([dunce::canonicalize(file).unwrap()]),
                watched_paths: HashSet::new(),
            },
        )
        .unwrap();

        assert!(!result);
    }

    #[test]
    fn rerun_ignored_for_explicitly_ignored_prebuild_output_with_dot_segments() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        let target_dir = root.join(BUILD_DIR);
        std::fs::create_dir_all(&target_dir).unwrap();

        let file = root.join("src/lib/generated.mbt");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "fn generated() {}").unwrap();

        let event = build_event(&file);
        let result = check_rerun_trigger(
            &target_dir,
            root,
            &[event],
            &AdditionalWatchPaths {
                ignored_paths: HashSet::from_iter([root.join("./src/lib/generated.mbt")]),
                watched_paths: HashSet::new(),
            },
        )
        .unwrap();

        assert!(!result);
    }

    #[test]
    fn rerun_target_dir_recreated_when_missing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        let target_dir = root.join(BUILD_DIR);

        let file = root.join("src/main.mbt");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "stuff").unwrap();

        let event = build_event(&file);

        assert!(!target_dir.exists());

        let result = check_rerun_trigger(
            &target_dir,
            root,
            &[event],
            &AdditionalWatchPaths::default(),
        )
        .unwrap();

        assert!(result);
        assert!(target_dir.exists());
    }

    #[test]
    fn fallback_helper_prefers_primary_result() {
        let (value, backend) =
            try_with_fallback("primary", || Ok(7), "fallback", || Ok(9)).unwrap();

        assert_eq!(value, 7);
        assert_eq!(backend, "primary");
    }

    #[test]
    fn fallback_helper_uses_fallback_after_primary_error() {
        let (value, backend) = try_with_fallback(
            "primary",
            || Err(anyhow!("primary failed")),
            "fallback",
            || Ok(9),
        )
        .unwrap();

        assert_eq!(value, 9);
        assert_eq!(backend, "fallback");
    }

    #[test]
    fn fallback_helper_reports_both_errors() {
        let error = try_with_fallback::<i32>(
            "primary",
            || Err(anyhow!("primary failed")),
            "fallback",
            || Err(anyhow!("fallback failed")),
        )
        .unwrap_err();

        let rendered = format!("{error:#}");
        assert!(rendered.contains("primary failed"));
        assert!(rendered.contains("fallback failed"));
    }

    #[test]
    fn metadata_write_time_on_file_is_relevant() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("src/main.mbt");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "stuff").unwrap();

        assert!(is_event_relevant(&metadata_write_time_event(&file)));
    }

    #[test]
    fn metadata_write_time_on_directory_is_ignored() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dir = temp_dir.path().join("src");
        std::fs::create_dir_all(&dir).unwrap();

        assert!(!is_event_relevant(&metadata_write_time_event(&dir)));
    }
}
