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
//! or `MAX_ROUNDS` is reached, followed by a check of the final contents.
//! `--diff` previews only the first round without applying fixes.

mod driver;

#[cfg(test)]
mod tests;

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

/// Apply compiler-proposed source fixes (experimental)
///
/// Runs the same check as `moon check` and applies the fixes attached to its
/// diagnostics, but only those that still match the files and pass the fix
/// checks; any other fix is skipped as a whole. A fix is applied only if every
/// target backend that compiles its file proposes it. Pass `--verbose` to see
/// why a fix was skipped.
///
/// Files select only themselves; directories select their package. Declared
/// prebuild outputs are excluded. Writes are atomic per file; a failed run may
/// retain files changed earlier. Exit 0 means the fix workflow completed, even
/// if compiler diagnostics remain. At most four rounds write files, followed
/// by a check of the final contents.
#[derive(Debug, clap::Parser, Clone)]
pub(crate) struct FixSubcommand {
    /// Package directory or source file to fix (file paths restrict edits to that file)
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

    /// Preview only the first round as a unified diff; later rounds may propose further changes
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
    // Discovery canonicalizes source directories. Resolve the display base
    // the same way, including Windows directory aliases and casing.
    let base = dunce::canonicalize(
        std::env::current_dir().context("failed to get the current directory")?,
    )
    .context("failed to resolve the current directory")?;
    let mut applied = 0;
    let mut files = BTreeSet::new();
    let mut failed = false;
    let mut last = None;
    // The extra iteration is check-only: every write, including the last
    // allowed round or a partially failed batch, gets a fresh validation.
    for round in 0..=MAX_ROUNDS {
        let collected = match run_check_collecting(cli, &check, output) {
            Ok(collected) => collected,
            Err(error) => {
                user_log.error(format!("failed to check for fixes: {error:#}"));
                failed = true;
                break;
            }
        };
        if cli.dry_run {
            return Ok(0);
        }
        if collected.execution_failed {
            user_log.error("check execution failed; no fixes from this check were applied");
            failed = true;
            break;
        }
        let plan = plan_collected_fixes(&collected);
        if cmd.diff {
            output.write_result(|writer| write_diff(writer, &plan, &base))?;
            files.extend(plan.changes.keys().cloned());
            applied = plan.applied_fix_count();
            last = Some((collected, plan));
            break;
        }
        if round == MAX_ROUNDS || failed || plan.changes.is_empty() {
            last = Some((collected, plan));
            break;
        }
        let outcome = write_changes(&plan, user_log);
        output.write_result(|writer| write_applied(writer, &plan, &outcome.written, &base))?;
        applied += outcome
            .written
            .iter()
            .map(|path| plan.changes[path].fixes.len())
            .sum::<usize>();
        failed |= outcome.failed;
        if outcome.written.is_empty() {
            // No source was changed in this round, so do not repeat the check.
            // A failed write is reported below without claiming final diagnostics.
            last = Some((collected, plan));
            break;
        }
        files.extend(outcome.written);
    }
    let plan = last.as_ref().map(|(_, plan)| plan);
    let skipped = plan.map(|plan| plan.skipped.as_slice()).unwrap_or_default();

    for skipped in skipped {
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
        (0, _) if failed => "No fixes applied".to_owned(),
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
    if !skipped.is_empty() {
        let n = skipped.len();
        let hint = if user_log.is_enabled(log::Level::Info) {
            ""
        } else {
            " (run with `--verbose` to see why)"
        };
        user_log.status(format!("Skipped {n} {}{hint}", fixes(n)));
    }
    if !cmd.diff
        && !failed
        && let Some((collected, plan)) = last
    {
        let (errors, warnings) = count_levels(&collected);
        if errors + warnings > 0 {
            user_log.status(format!(
                "{errors} errors and {warnings} warnings remain; run `moon check` to see them"
            ));
        }
        if !plan.changes.is_empty() {
            user_log.status("More fixes may apply; run `moon fix` again");
        }
    }
    Ok(i32::from(failed))
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
    plan_fixes(&proposals, &collected.editable_sources, |path| {
        std::fs::read_to_string(path)
    })
}

struct WriteOutcome {
    written: BTreeSet<PathBuf>,
    failed: bool,
}

/// Commit one complete file at a time and stop on the first failure. Files
/// committed before it are retained and reported; the batch is not a transaction.
fn write_changes(plan: &FixPlan, user_log: &moonutil::user_log::UserLog) -> WriteOutcome {
    let mut outcome = WriteOutcome {
        written: BTreeSet::new(),
        failed: false,
    };
    for (path, change) in &plan.changes {
        let result = (|| -> std::io::Result<()> {
            if std::fs::read_to_string(path)? != change.original {
                return Err(std::io::Error::other("source changed during the check"));
            }
            // Follow a source symlink without replacing the link itself.
            // Opening without truncation checks write access before staging;
            // rename alone could otherwise overwrite a read-only source.
            let destination = std::fs::canonicalize(path)?;
            let original = std::fs::OpenOptions::new().write(true).open(&destination)?;
            let permissions = original.metadata()?.permissions();
            if permissions.readonly() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "source is read-only",
                ));
            }
            drop(original);
            let mut staged = tempfile::NamedTempFile::new_in(
                destination.parent().expect("source file has a parent"),
            )?;
            staged.write_all(change.fixed.as_bytes())?;
            staged.as_file().set_permissions(permissions)?;
            staged.as_file().sync_all()?;
            if std::fs::read_to_string(&destination)? != change.original {
                return Err(std::io::Error::other("source changed during the check"));
            }
            staged.persist(&destination).map_err(|error| error.error)?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                outcome.written.insert(path.clone());
            }
            Err(error) => {
                user_log.error(format!("failed to fix `{}`: {error}", path.display()));
                outcome.failed = true;
                break;
            }
        }
    }
    outcome
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
    let path = dunce::simplified(path);
    path.strip_prefix(dunce::simplified(base))
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}

fn count_levels(collected: &CollectedCheck) -> (usize, usize) {
    collected
        .diagnostics
        .iter()
        .fold((0, 0), |(errors, warnings), diagnostic| {
            match diagnostic
                .value
                .get("level")
                .and_then(|level| level.as_str())
            {
                Some("error") => (errors + 1, warnings),
                Some("warn" | "warning") => (errors, warnings + 1),
                _ => (errors, warnings),
            }
        })
}
