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

//! `moon fix`: apply the machine-applicable fixes of check diagnostics.
//!
//! Each round runs the same check as `moon check` with JSON diagnostics,
//! collects the preferred fix of every diagnostic per Target Backend, and lets
//! the [fix driver](driver) select the fixes that pass its checks against the
//! files on disk. Fixes describe those files, so the check never uses patch
//! or replaced-content inputs.
//!
//! Applying a fix can make others applicable: one skipped because it touched
//! an applied fix, or one in a package that failed to check before. Rounds
//! therefore repeat, each with a fresh check, until a round applies nothing
//! or `MAX_ROUNDS` is reached. `--diff` writes nothing and so runs one round.

mod driver;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use moonutil::cli_support::{AutoSyncFlags, UniversalFlags};
use moonutil::command_output::CommandOutput;
use moonutil::target::SurfaceTarget;
use tracing::instrument;

use super::BuildFlags;
use super::check::{CheckSubcommand, CollectedCheck, run_check_collecting};
use driver::{Fix, FixPlan, plan_fixes};

const MAX_ROUNDS: usize = 4;

/// Apply the machine-applicable fixes that check diagnostics propose
///
/// Runs the same check as `moon check` and applies the fixes attached to its
/// diagnostics, but only those that still match the files and pass the fix
/// checks; any other fix is skipped as a whole. A fix is applied only if every
/// target backend that compiles its file proposes it. Pass `--verbose` to see
/// why a fix was skipped.
#[derive(Debug, clap::Parser, Clone)]
pub(crate) struct FixSubcommand {
    /// Filesystem path to a package directory, `.mbt` / `.mbt.md` selector, or standalone `.mbtx` file
    #[clap(name = "PATH")]
    pub path: Vec<PathBuf>,

    /// Select the target backends to check
    #[clap(long, value_delimiter = ',')]
    pub target: Vec<SurfaceTarget>,

    /// Warn list config
    #[clap(long, allow_hyphen_values = true)]
    pub warn_list: Option<String>,

    /// Set the max number of jobs to run in parallel
    #[clap(short = 'j', long)]
    pub jobs: Option<usize>,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Print the changes as a unified diff instead of writing them
    #[clap(long)]
    pub diff: bool,
}

impl FixSubcommand {
    fn check_command(&self) -> CheckSubcommand {
        CheckSubcommand {
            build_flags: BuildFlags {
                target: self.target.clone(),
                warn_list: self.warn_list.clone(),
                jobs: self.jobs,
                ..BuildFlags::default()
            },
            auto_sync_flags: self.auto_sync_flags.clone(),
            watch: false,
            package_path: None,
            // Fixes describe the files on disk.
            patch_file: None,
            explain: false,
            path: self.path.clone(),
            fmt: false,
            json: false,
            // Interfaces of the code before its fixes would be stale.
            no_mbti: true,
        }
    }
}

#[instrument(skip_all)]
pub(crate) fn run_fix(
    cli: &UniversalFlags,
    cmd: &FixSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let user_log = output.user_log();
    let check = cmd.check_command();
    let base = std::env::current_dir().context("failed to get the current directory")?;
    let rounds = if cmd.diff || cli.dry_run {
        1
    } else {
        MAX_ROUNDS
    };

    let mut applied = 0;
    let mut files = BTreeSet::new();
    let mut last = None;
    for _ in 0..rounds {
        let collected = run_check_collecting(cli, &check, output)?;
        let plan = plan_collected_fixes(&collected);
        let round_applied = if cmd.diff {
            output.write_result(|writer| write_diff(writer, &plan, &base))?;
            files.extend(plan.changes.keys().cloned());
            plan.applied_fix_count()
        } else {
            let written = write_changes(&plan, user_log);
            output.write_result(|writer| write_applied(writer, &plan, &written, &base))?;
            let n = written
                .iter()
                .map(|path| plan.changes[path].fixes.len())
                .sum();
            files.extend(written);
            n
        };
        applied += round_applied;
        last = Some((collected, plan, round_applied));
        if round_applied == 0 {
            break;
        }
    }
    if cli.dry_run {
        return Ok(0);
    }
    let (collected, plan, last_applied) = last.expect("at least one round runs");

    for skipped in &plan.skipped {
        user_log.info(format!(
            "skipped fix `{}`{}: {}",
            skipped.fix.title,
            fix_location(&skipped.fix, &base)
                .map(|location| format!(" at {location}"))
                .unwrap_or_default(),
            skipped.reason
        ));
    }
    let fixes = |n: usize| if n == 1 { "fix" } else { "fixes" };
    let in_files = |n: usize| if n == 1 { "file" } else { "files" };
    user_log.status(match (applied, cmd.diff) {
        (0, _) => "No fixes to apply".to_owned(),
        (n, true) => format!(
            "Would apply {n} {} to {} {}",
            fixes(n),
            files.len(),
            in_files(files.len())
        ),
        (n, false) => format!(
            "Applied {n} {} to {} {}",
            fixes(n),
            files.len(),
            in_files(files.len())
        ),
    });
    if !plan.skipped.is_empty() {
        let n = plan.skipped.len();
        let hint = if user_log.is_enabled(log::Level::Info) {
            ""
        } else {
            " (run with `--verbose` to see why)"
        };
        user_log.status(format!("Skipped {n} {}{hint}", fixes(n)));
    }
    if cmd.diff {
        // The check saw the files before the diff.
    } else if last_applied == 0 {
        // The last check saw the files as they are now.
        let (errors, warnings) = count_levels(&collected);
        if errors + warnings > 0 {
            user_log.status(format!(
                "{errors} errors and {warnings} warnings remain; run `moon check` to see them"
            ));
        }
    } else {
        user_log.status("More fixes may apply; run `moon fix` again");
    }
    Ok(0)
}

/// Run the fix driver on the diagnostics of one check.
fn plan_collected_fixes(collected: &CollectedCheck) -> FixPlan {
    let mut proposals: BTreeMap<_, BTreeSet<Fix>> = BTreeMap::new();
    for diagnostic in &collected.diagnostics {
        if let Some(fix) = Fix::preferred_of_diagnostic(&diagnostic.value) {
            proposals
                .entry(diagnostic.target_backend)
                .or_default()
                .insert(fix);
        }
    }
    plan_fixes(&proposals, &collected.compiled_sources, |path| {
        std::fs::read_to_string(path)
    })
}

/// Write the changed files and return those written.
///
/// A file that changed since the driver read it is left alone; the next check
/// sees it as it is.
fn write_changes(plan: &FixPlan, user_log: &moonutil::user_log::UserLog) -> BTreeSet<PathBuf> {
    let mut written = BTreeSet::new();
    for (path, change) in &plan.changes {
        let result = std::fs::read_to_string(path).and_then(|current| {
            if current != change.original {
                return Ok(false);
            }
            std::fs::write(path, &change.fixed).map(|()| true)
        });
        match result {
            Ok(true) => {
                written.insert(path.clone());
            }
            Ok(false) => user_log.warn(format!(
                "not fixing `{}`: it changed during the check",
                path.display()
            )),
            Err(e) => user_log.warn(format!("failed to fix `{}`: {e}", path.display())),
        }
    }
    written
}

/// The Command Result of a round that writes: one line per applied fix.
fn write_applied(
    writer: &mut dyn Write,
    plan: &FixPlan,
    written: &BTreeSet<PathBuf>,
    base: &Path,
) -> std::io::Result<()> {
    for (path, change) in &plan.changes {
        if !written.contains(path) {
            continue;
        }
        for fix in &change.fixes {
            let location = fix_location(fix, base).unwrap_or_default();
            writeln!(writer, "{location}: {}", fix.title)?;
        }
    }
    Ok(())
}

/// The Command Result of `--diff`: a unified diff of every changed file.
fn write_diff(writer: &mut dyn Write, plan: &FixPlan, base: &Path) -> std::io::Result<()> {
    for (path, change) in &plan.changes {
        let name = display_path(path, base);
        let diff = similar::TextDiff::from_lines(&change.original, &change.fixed);
        write!(
            writer,
            "{}",
            diff.unified_diff()
                .header(&format!("a/{name}"), &format!("b/{name}"))
        )?;
    }
    Ok(())
}

/// `path:line:column` of the first edit of a fix.
fn fix_location(fix: &Fix, base: &Path) -> Option<String> {
    let edit = fix.edits.first()?;
    let start = edit.loc.split('-').next().unwrap_or(&edit.loc);
    Some(format!("{}:{start}", display_path(&edit.path, base)))
}

fn display_path(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}

fn count_levels(collected: &CollectedCheck) -> (usize, usize) {
    let level = |d: &moonbuild::execution::JsonBuildDiagnostic| {
        d.value
            .get("level")
            .and_then(|level| level.as_str())
            .map(str::to_owned)
    };
    let errors = collected
        .diagnostics
        .iter()
        .filter(|d| level(d).as_deref() == Some("error"))
        .count();
    let warnings = collected
        .diagnostics
        .iter()
        .filter(|d| level(d).as_deref() == Some("warning"))
        .count();
    (errors, warnings)
}
