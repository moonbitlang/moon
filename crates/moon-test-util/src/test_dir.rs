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

use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::Context;

pub fn copy_tree(src: &Path, dest: &Path, exclude_target: bool) -> anyhow::Result<()> {
    if src.is_dir() {
        if !dest.exists() {
            std::fs::create_dir_all(dest)?;
        }
        let mut walker = ignore::WalkBuilder::new(src);
        walker.hidden(false);
        walker.git_global(false);
        if exclude_target {
            walker.filter_entry(|x| x.file_name() != "target");
        }
        for entry in walker.build() {
            let entry = entry?;
            let path = entry.path();
            let relative_path = path.strip_prefix(src)?;
            let dest_path = dest.join(relative_path);
            if path.is_dir() {
                if !dest_path.exists() {
                    std::fs::create_dir_all(dest_path)?;
                }
            } else {
                std::fs::copy(path, dest_path)?;
            }
        }
    } else {
        std::fs::copy(src, dest)?;
    }
    Ok(())
}

pub struct TestDir {
    // Taken on drop so cleanup can be timed, or explicitly disabled for inspection.
    path: Option<tempfile::TempDir>,
    trace: Option<Trace>,
    retain: bool,
}

impl TestDir {
    pub fn from_case_root(
        case_root: impl AsRef<Path>,
        sub: impl AsRef<Path>,
        exclude_target: bool,
    ) -> Self {
        Self::prepare(
            Some(&case_root.as_ref().join(sub)),
            exclude_target,
            Options::from_env(),
        )
    }

    pub fn new_empty() -> Self {
        Self::prepare(None, false, Options::from_env())
    }

    /// Write inline fixture files, creating their parent directories as needed.
    /// Paths must be relative to this workspace. The writes form a `setup`
    /// trace phase; failures clean up or retain partially initialized files
    /// according to the same policy as copied fixtures.
    pub fn with_files(
        mut self,
        files: impl IntoIterator<Item = (impl AsRef<Path>, impl AsRef<[u8]>)>,
    ) -> Self {
        if let Some(trace) = &mut self.trace {
            trace.record(serde_json::json!({"phase": "setup", "state": "started"}));
        }
        let started = Instant::now();
        let initialized = files.into_iter().try_for_each(|(relative, contents)| {
            let relative = relative.as_ref();
            anyhow::ensure!(
                !relative.as_os_str().is_empty()
                    && relative.components().all(|part| matches!(
                        part,
                        std::path::Component::Normal(_) | std::path::Component::CurDir
                    )),
                "fixture file path must be relative without parent components: {}",
                relative.display()
            );
            let path = self.join(relative);
            if let Some(parent) = relative
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                std::fs::create_dir_all(self.join(parent))
                    .with_context(|| format!("create parent directory for {}", path.display()))?;
            }
            std::fs::write(&path, contents)
                .with_context(|| format!("write test fixture file {}", path.display()))
        });
        let elapsed = started.elapsed();
        if let Some(trace) = &mut self.trace {
            trace.record(serde_json::json!({
                "phase": "setup", "state": if initialized.is_ok() { "ok" } else { "error" },
                "elapsed_us": elapsed.as_micros(),
                "error": initialized.as_ref().err().map(|error| format!("{error:#}")),
            }));
        }
        initialized.expect("initialize test fixture files");
        self
    }

    fn prepare(source: Option<&Path>, exclude_target: bool, options: Options) -> Self {
        let mut trace = options
            .trace_dir
            .as_deref()
            .and_then(|root| Trace::open(root, source));
        if let Some(trace) = &mut trace {
            trace.record(serde_json::json!({"phase": "create", "state": "started"}));
        }
        let started = Instant::now();
        let tmp_dir = match &options.keep_dir {
            Some(root) => std::fs::create_dir_all(root).and_then(|()| {
                let root = dunce::canonicalize(root)?;
                tempfile::Builder::new()
                    .prefix("moon-test-")
                    .tempdir_in(root)
            }),
            None => tempfile::TempDir::new(),
        };
        let elapsed = started.elapsed();
        if let Some(trace) = &mut trace {
            trace.workspace = tmp_dir.as_ref().ok().map(|dir| dir.path().to_owned());
            trace.record(serde_json::json!({
                "phase": "create", "state": if tmp_dir.is_ok() { "ok" } else { "error" },
                "elapsed_us": elapsed.as_micros(),
                "error": tmp_dir.as_ref().err().map(ToString::to_string),
            }));
        }
        // Construct the owner before copying so a setup panic also cleans up or
        // retains the partial workspace according to the requested policy.
        let mut dir = Self {
            path: Some(tmp_dir.expect("create temp dir for tests")),
            trace,
            retain: options.keep_dir.is_some(),
        };
        let Some(source) = source else { return dir };
        if let Some(trace) = &mut dir.trace {
            trace.record(serde_json::json!({"phase": "copy", "state": "started"}));
        }
        let started = Instant::now();
        let copied = copy_tree(source, dir.as_ref(), exclude_target);
        let elapsed = started.elapsed();
        if let Some(trace) = &mut dir.trace {
            trace.record(serde_json::json!({
                "phase": "copy", "state": if copied.is_ok() { "ok" } else { "error" },
                "elapsed_us": elapsed.as_micros(),
                "error": copied.as_ref().err().map(ToString::to_string),
            }));
        }
        copied.expect("copy test case to temp dir");
        dir
    }

    pub fn join(&self, sub: impl AsRef<Path>) -> PathBuf {
        self.as_ref().join(sub.as_ref())
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        self.path.as_ref().expect("live test directory").path()
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let Some(dir) = self.path.take() else {
            return;
        };
        if let Some(trace) = &mut self.trace {
            trace.record(serde_json::json!({"phase": "cleanup", "state": "started"}));
        }
        let started = Instant::now();
        let cleaned = if self.retain {
            let _retained_path = dir.into_path();
            Ok(())
        } else {
            dir.close()
        };
        let elapsed = started.elapsed();
        if let Some(trace) = &mut self.trace {
            trace.record(serde_json::json!({
                "phase": "cleanup",
                "state": if self.retain { "retained" } else if cleaned.is_ok() { "ok" } else { "error" },
                "elapsed_us": elapsed.as_micros(),
                "error": cleaned.as_ref().err().map(ToString::to_string),
            }));
        }
        // Like TempDir's destructor, cleanup must not turn a test failure into a
        // second panic. The trace exposes errors that Drop previously discarded.
        if let Err(error) = cleaned {
            tracing::warn!(%error, "failed to clean up test directory");
        }
    }
}

#[derive(Default)]
struct Options {
    trace_dir: Option<PathBuf>,
    keep_dir: Option<PathBuf>,
}

impl Options {
    fn from_env() -> Self {
        Self {
            trace_dir: std::env::var_os("MOON_TEST_TRACE_DIR")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
            keep_dir: std::env::var_os("MOON_TEST_KEEP_DIR")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
        }
    }
}

struct Trace {
    file: File,
    test: String,
    fixture: Option<PathBuf>,
    workspace: Option<PathBuf>,
}

impl Trace {
    fn open(root: &Path, fixture: Option<&Path>) -> Option<Self> {
        let opened = (|| -> anyhow::Result<File> {
            std::fs::create_dir_all(root)?;
            let (file, _) = tempfile::Builder::new()
                .prefix("test-dir-")
                .suffix(".jsonl")
                .tempfile_in(root)?
                .keep()?;
            Ok(file)
        })();
        match opened {
            Ok(file) => Some(Self {
                file,
                test: std::thread::current()
                    .name()
                    .unwrap_or("<unnamed>")
                    .to_owned(),
                fixture: fixture.map(Path::to_owned),
                workspace: None,
            }),
            Err(error) => {
                tracing::warn!(%error, "failed to create TestDir trace");
                None
            }
        }
    }

    fn record(&mut self, event: serde_json::Value) {
        let record = serde_json::json!({
            "pid": std::process::id(), "test": self.test,
            "fixture": self.fixture.as_deref().map(Path::to_string_lossy),
            "workspace": self.workspace.as_deref().map(Path::to_string_lossy), "event": event,
        });
        // One write per event, outside the measured phase. Avoid a shared log
        // lock and retain completed events if a later phase hangs or aborts.
        let written = serde_json::to_vec(&record)
            .map_err(std::io::Error::other)
            .and_then(|mut line| {
                line.push(b'\n');
                self.file.write_all(&line)
            });
        if let Err(error) = written {
            tracing::warn!(%error, "failed to write TestDir trace");
        }
    }
}

#[cfg(test)]
mod tests;
