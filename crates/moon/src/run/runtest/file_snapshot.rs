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

//! Candidate-file reports. Core compares bytes; Moon owns presentation and promotion.
use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, ensure};
use moonbuild_rupes_recta::discover::DiscoverResult;
use serde::Deserialize;

use super::{TestCaseResult, TestStatistics};

const PREFIX: &str = "@FILE_SNAPSHOTS ";
// Both the read and diff inputs are capped. Large or binary snapshots still
// participate in promotion, which streams their candidate files unchanged.
const MAX_DIFF_BYTES: u64 = 256 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Report {
    pub body_error: String,
    pub snapshots: Vec<Snapshot>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Snapshot {
    expected: String,
    actual: PathBuf,
    location: String,
    pub offset: String,
}

pub(super) fn parse(message: &str) -> anyhow::Result<Option<Report>> {
    message
        .strip_prefix(PREFIX)
        .map(|body| serde_json_lenient::from_str(body).context("invalid candidate-file report"))
        .transpose()
}

fn valid_name(name: &str) -> bool {
    !name.contains(['\\', ':', '\0'])
        && name.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

fn baseline_root(packages: &DiscoverResult, result: &TestStatistics) -> anyhow::Result<PathBuf> {
    let name = result
        .package
        .strip_suffix("_blackbox_test")
        .unwrap_or(&result.package);
    let id = packages
        .get_package_id_by_name(name)
        .context("unknown snapshot package")?;
    Ok(packages.get_package(id).root_path.join("__snapshot__"))
}

impl Report {
    pub fn validate(&self, artifacts: &Path) -> anyhow::Result<()> {
        let mut names = HashSet::new();
        let mut candidates = HashSet::new();
        for snapshot in &self.snapshots {
            ensure!(
                valid_name(&snapshot.expected),
                "invalid snapshot baseline name"
            );
            ensure!(
                names.insert(&snapshot.expected),
                "duplicate snapshot baseline: {}",
                snapshot.expected
            );
            let offset: i64 = snapshot.offset.parse().context("invalid snapshot offset")?;
            ensure!(offset >= -2, "invalid snapshot comparison result");
            let actual = dunce::canonicalize(&snapshot.actual).context("missing candidate file")?;
            ensure!(
                actual.parent() == Some(artifacts) && actual.is_file(),
                "candidate must be a file in this invocation's artifact directory"
            );
            ensure!(
                candidates.insert(actual),
                "candidate submitted more than once"
            );
        }
        Ok(())
    }

    pub fn render(&self, packages: &DiscoverResult, result: &TestStatistics) -> anyhow::Result<()> {
        let root = baseline_root(packages, result)?;
        for snapshot in &self.snapshots {
            if snapshot.offset == "-1" {
                continue;
            }
            let expected = root.join(&snapshot.expected);
            println!(
                "Snapshot {} at {}: {}",
                snapshot.expected,
                snapshot.location,
                if snapshot.offset == "-2" {
                    "baseline missing".into()
                } else {
                    format!("first difference at byte {}", snapshot.offset)
                }
            );
            println!("Actual output: {}", snapshot.actual.display());
            match (
                read_diff_text(&expected, true)?,
                read_diff_text(&snapshot.actual, false)?,
            ) {
                (Some(old), Some(new)) => {
                    crate::diff::write_diff(&old, &new, 20, 5, &mut std::io::stdout())?
                }
                _ => println!("Binary or large snapshot; textual diff omitted."),
            }
        }
        Ok(())
    }
}

fn read_diff_text(path: &Path, allow_missing: bool) -> anyhow::Result<Option<String>> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if allow_missing && e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Some(String::new()));
        }
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_DIFF_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_DIFF_BYTES || bytes.contains(&0) {
        return Ok(None);
    }
    Ok(String::from_utf8(bytes).ok())
}

/// Check even passing and errored tests: their baseline claims must not be
/// silently overwritten by a different test selected for promotion.
pub(super) fn validate_claims<'a>(
    packages: &DiscoverResult,
    results: impl Iterator<Item = &'a TestCaseResult>,
) -> anyhow::Result<()> {
    let mut names = HashSet::new();
    for result in results {
        let Some(report) = parse(&result.raw.message)? else {
            continue;
        };
        let root = baseline_root(packages, &result.raw)?;
        for snapshot in report.snapshots {
            let expected = root.join(snapshot.expected);
            ensure!(
                names.insert(expected.clone()),
                "conflicting snapshot baseline: {}",
                expected.display()
            );
        }
    }
    Ok(())
}

pub(super) fn promote(
    packages: &DiscoverResult,
    results: &[&TestCaseResult],
) -> anyhow::Result<()> {
    let mut outputs = Vec::new();
    let mut names = HashSet::new();
    // Validate the whole batch before the first write. Same-baseline updates
    // from distinct tests are ambiguous even if their bytes currently match.
    for result in results {
        let Some(report) = parse(&result.raw.message)? else {
            continue;
        };
        ensure!(
            report.body_error.is_empty(),
            "cannot promote a test with a body error"
        );
        let root = baseline_root(packages, &result.raw)?;
        for snapshot in report.snapshots {
            ensure!(
                valid_name(&snapshot.expected),
                "invalid snapshot baseline name"
            );
            let expected = root.join(&snapshot.expected);
            ensure!(
                names.insert(expected.clone()),
                "conflicting snapshot baseline: {}",
                expected.display()
            );
            if snapshot.offset != "-1" {
                outputs.push((snapshot.actual, expected));
            }
        }
    }
    for (actual, expected) in outputs {
        let parent = expected.parent().context("snapshot has no parent")?;
        std::fs::create_dir_all(parent)?;
        // Stage beside the destination for per-file atomic replacement. This is
        // not an all-files transaction; an I/O failure may follow earlier updates.
        let mut staged = tempfile::NamedTempFile::new_in(parent)?;
        std::io::copy(&mut std::fs::File::open(&actual)?, &mut staged)?;
        staged
            .persist(&expected)
            .with_context(|| format!("promote snapshot {}", expected.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_validates_files_and_names() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let actual = root.join("candidate");
        std::fs::write(&actual, b"actual").unwrap();
        let mut report = Report {
            body_error: String::new(),
            snapshots: vec![Snapshot {
                actual,
                expected: "nested/a.bin".into(),
                location: "test.mbt:1".into(),
                offset: "-2".into(),
            }],
        };
        report.validate(&root).unwrap();
        report.snapshots.push(Snapshot {
            actual: root.join("candidate"),
            expected: "other".into(),
            location: String::new(),
            offset: "-1".into(),
        });
        assert!(report.validate(&root).is_err());
        report.snapshots.pop();
        let outside = tempfile::NamedTempFile::new().unwrap();
        report.snapshots[0].actual = outside.path().to_owned();
        assert!(report.validate(&root).is_err());
        report.snapshots[0].actual = root.join("candidate");
        report.snapshots[0].expected = "../escape".into();
        assert!(report.validate(&root).is_err());
        report.snapshots[0].expected = "valid".into();
        report.snapshots[0].offset = "-3".into();
        assert!(report.validate(&root).is_err());
    }

    #[test]
    fn diff_reads_are_bounded_and_binary_aware() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data");
        assert_eq!(read_diff_text(&path, true).unwrap(), Some(String::new()));
        assert!(read_diff_text(&path, false).is_err());
        std::fs::write(&path, b"\0binary").unwrap();
        assert_eq!(read_diff_text(&path, false).unwrap(), None);
        std::fs::write(&path, vec![b'x'; MAX_DIFF_BYTES as usize + 1]).unwrap();
        assert_eq!(read_diff_text(&path, false).unwrap(), None);
        std::fs::write(&path, "text\n").unwrap();
        assert_eq!(read_diff_text(&path, false).unwrap(), Some("text\n".into()));
    }
}
