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

//! One-shot artifact delivery from a Moon helper to its owning moonrun.

use std::fs::{self, File};
use std::path::{Path, PathBuf};

use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};

/// How the supervisor executes the delivered program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionKind {
    Wasm,
    Native,
    Js,
}

impl ExecutionKind {
    fn artifact_name(self) -> &'static str {
        match self {
            Self::Wasm => "program.wasm",
            Self::Native => "program.exe",
            Self::Js => "program.js",
        }
    }
}

pub const HANDOFF_ARG: &str = "--moonrun-artifact-handoff";

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Request {
    inherited_policy: bool,
}

/// The caller owns this private directory until the resulting Run finishes.
/// Only final artifacts are copied here; build caches retain their usual owners.
#[derive(Clone, Debug)]
pub struct RunHandoff {
    directory: PathBuf,
    request: Request,
}

#[derive(Deserialize, Serialize)]
struct Manifest {
    execution_kind: ExecutionKind,
    executable: PathBuf,
    args: Vec<String>,
    working_directory: PathBuf,
    policy: Option<PolicySource>,
    source_map: bool,
}

#[derive(Deserialize, Serialize)]
struct PolicySource {
    mbtx: bool,
    source_dir: PathBuf,
}

pub struct PreparedProgram {
    pub execution_kind: ExecutionKind,
    /// Owned copy, retained until the supervised process finishes.
    pub artifact: PathBuf,
    pub source_map: Option<String>,
    /// Original executable identity for guest paths and diagnostics.
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub working_directory: PathBuf,
    pub policy: Option<(PathBuf, PathBuf)>,
}

impl RunHandoff {
    pub fn new(directory: &Path, inherited_policy: bool) -> anyhow::Result<Self> {
        let directory = dunce::canonicalize(directory)?;
        let request = Request { inherited_policy };
        serde_json_lenient::to_writer(File::create(directory.join("request.json"))?, &request)?;
        Ok(Self { directory, request })
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn open(directory: &Path) -> anyhow::Result<Self> {
        let directory = dunce::canonicalize(directory)?;
        let request = serde_json_lenient::from_reader(File::open(directory.join("request.json"))?)?;
        Ok(Self { directory, request })
    }

    pub fn publish(
        &self,
        executable: &Path,
        execution_kind: ExecutionKind,
        args: &[String],
        policy: Option<&Path>,
        policy_source_dir: Option<&Path>,
    ) -> anyhow::Result<()> {
        let executable = std::path::absolute(executable)?;
        let source_map = if matches!(execution_kind, ExecutionKind::Wasm) {
            let contents = fs::read(&executable)?;
            ensure!(contents.starts_with(b"\0asm"), "invalid Wasm artifact");
            crate::wasm_source_map::load(&executable, &contents)
        } else {
            None
        };
        // Copy before the producer releases build locks or temporary sources.
        // fs::copy also retains permissions for future native execution.
        fs::copy(
            &executable,
            self.directory.join(execution_kind.artifact_name()),
        )?;
        if let Some(source_map) = &source_map {
            fs::write(self.directory.join("program.map"), source_map)?;
        }
        let policy = if self.request.inherited_policy {
            None
        } else {
            policy
                .map(|path| -> anyhow::Result<_> {
                    // Policy format and relative roots follow the invoked
                    // path, even when the policy itself is a symlink.
                    let path = std::path::absolute(path)?;
                    let mbtx = path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("mbtx"));
                    let source_dir = dunce::canonicalize(
                        policy_source_dir.unwrap_or_else(|| path.parent().unwrap()),
                    )?;
                    fs::copy(
                        &path,
                        self.directory
                            .join(if mbtx { "policy.mbtx" } else { "policy.conf" }),
                    )?;
                    Ok(PolicySource { mbtx, source_dir })
                })
                .transpose()?
        };
        // Publish last, while Moon still owns inline-source temporaries and
        // build locks. The consumer also requires successful helper exit.
        serde_json_lenient::to_writer(
            File::create(self.directory.join("result.json"))?,
            &Manifest {
                execution_kind,
                executable,
                args: args.to_vec(),
                working_directory: std::env::current_dir()?,
                policy,
                source_map: source_map.is_some(),
            },
        )?;
        Ok(())
    }

    pub fn load(&self) -> anyhow::Result<PreparedProgram> {
        let manifest: Manifest = serde_json_lenient::from_reader(
            File::open(self.directory.join("result.json"))
                .context("Moon helper did not return an executable artifact")?,
        )?;
        Ok(PreparedProgram {
            execution_kind: manifest.execution_kind,
            artifact: self.directory.join(manifest.execution_kind.artifact_name()),
            source_map: manifest
                .source_map
                .then(|| fs::read_to_string(self.directory.join("program.map")))
                .transpose()?,
            executable: manifest.executable,
            args: manifest.args,
            working_directory: manifest.working_directory,
            policy: manifest.policy.map(|policy| {
                (
                    self.directory.join(if policy.mbtx {
                        "policy.mbtx"
                    } else {
                        "policy.conf"
                    }),
                    policy.source_dir,
                )
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_and_js_delivery_uses_the_same_owned_artifact_contract() {
        for kind in [ExecutionKind::Native, ExecutionKind::Js] {
            let source = tempfile::tempdir().unwrap();
            let executable = source.path().join(kind.artifact_name());
            fs::write(&executable, b"non-Wasm artifact").unwrap();
            let directory = tempfile::tempdir().unwrap();
            let handoff = RunHandoff::new(directory.path(), false).unwrap();
            handoff
                .publish(&executable, kind, &["argument".into()], None, None)
                .unwrap();
            source.close().unwrap();
            let prepared = handoff.load().unwrap();
            assert_eq!(prepared.execution_kind, kind);
            assert_eq!(fs::read(prepared.artifact).unwrap(), b"non-Wasm artifact");
            assert_eq!(prepared.args, ["argument"]);
            assert!(prepared.source_map.is_none());
        }
    }

    #[test]
    fn delivery_owns_artifact_and_source_map_after_source_cleanup() {
        let source = tempfile::tempdir().unwrap();
        let wasm = source.path().join("program.wasm");
        let bytes = b"\0asm\x01\0\0\0";
        let map = r#"{"sources":["original.mbt"],"mappings":"AAAA"}"#;
        fs::write(&wasm, bytes).unwrap();
        fs::write(source.path().join("program.wasm.map"), map).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let handoff = RunHandoff::new(directory.path(), false).unwrap();
        handoff
            .publish(&wasm, ExecutionKind::Wasm, &["hello".into()], None, None)
            .unwrap();
        source.close().unwrap();
        let prepared = handoff.load().unwrap();
        assert_eq!(fs::read(&prepared.artifact).unwrap(), bytes);
        assert_eq!(
            fs::read_to_string(directory.path().join("program.map")).unwrap(),
            map
        );
        assert_eq!(prepared.executable, wasm);
        assert_eq!(prepared.args, ["hello"]);
    }

    #[cfg(unix)]
    #[test]
    fn policy_symlinks_keep_the_invoked_base_directory_and_format() {
        let root = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let source = elsewhere.path().join("source.mbtx");
        fs::write(&source, "[fs]\nread = ['.']\n").unwrap();
        // The alias is TOML, despite the target's extension. Both its format
        // and relative filesystem roots must match ordinary policy loading.
        let alias = root.path().join("policy.toml");
        std::os::unix::fs::symlink(source, &alias).unwrap();
        let wasm = root.path().join("guest.wasm");
        fs::write(&wasm, b"\0asm\x01\0\0\0").unwrap();
        let directory = tempfile::tempdir().unwrap();
        let handoff = RunHandoff::new(directory.path(), false).unwrap();
        handoff
            .publish(&wasm, ExecutionKind::Wasm, &[], Some(&alias), None)
            .unwrap();
        let (policy, base) = handoff.load().unwrap().policy.unwrap();
        assert_eq!(policy.extension().unwrap(), "conf");
        assert_eq!(fs::read_to_string(policy).unwrap(), "[fs]\nread = ['.']\n");
        assert_eq!(base, dunce::canonicalize(root.path()).unwrap());
    }
}
