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

//! Complete registry dependency graphs cached for standalone script execution.
//!
//! n2 still schedules every cold dependency action together. On a hit this
//! module restores the complete `.mi`/`.core` output set into the invocation's
//! target directory, after which those producer nodes are detached from the
//! project-local n2 graph.

use std::{
    collections::HashMap,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use moonbuild_rupes_recta::dependency_build_cache::DependencyBuildAction;
use moonutil::{
    cache::{CacheKind, CacheRoot, initialize_cache_root, resolve_cache_root},
    locks::FileLock,
};
use n2::graph::{FileId, Graph};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CACHE_FORMAT: &str = "moon-script-dependency-graph-v1";

#[derive(Debug)]
pub(crate) struct DependencyGraphCache {
    root: PathBuf,
    graph_id: String,
    build_ids: Vec<n2::graph::BuildId>,
    outputs: Vec<OutputSpec>,
}

#[derive(Debug)]
struct OutputSpec {
    label: String,
    path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct GraphRecord {
    format: String,
    outputs: Vec<CachedOutput>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CachedOutput {
    label: String,
    file: String,
    digest: String,
    size: u64,
}

impl DependencyGraphCache {
    pub(crate) fn open(actions: &[DependencyBuildAction]) -> anyhow::Result<Option<Self>> {
        if actions.is_empty() {
            return Ok(None);
        }
        let CacheRoot::Path(root) = resolve_cache_root(CacheKind::BuildArtifacts)? else {
            return Ok(None);
        };
        initialize_cache_root(CacheKind::BuildArtifacts, &root)?;

        let graph_id = dependency_graph_id(actions)?;
        let mut descriptions = actions.iter().collect::<Vec<_>>();
        descriptions.sort_by(|left, right| {
            left.description
                .package
                .cmp(&right.description.package)
                .then_with(|| {
                    left.description
                        .canonical_args
                        .cmp(&right.description.canonical_args)
                })
        });
        let mut outputs = descriptions
            .iter()
            .flat_map(|action| &action.description.outputs)
            .map(|output| OutputSpec {
                label: output.label.clone(),
                path: output.path.clone(),
            })
            .collect::<Vec<_>>();
        outputs.sort_by(|left, right| left.label.cmp(&right.label));

        Ok(Some(Self {
            root,
            graph_id,
            build_ids: actions.iter().map(|action| action.build_id).collect(),
            outputs,
        }))
    }

    pub(crate) fn lock(&self) -> anyhow::Result<FileLock> {
        let directory = sharded_path(&self.root.join("locks/dependency-graphs"), &self.graph_id);
        std::fs::create_dir_all(&directory)?;
        Ok(FileLock::lock_with_verbosity(&directory, false)?)
    }

    pub(crate) fn restore(&self) -> anyhow::Result<bool> {
        let entry = self.entry_path();
        match std::fs::symlink_metadata(&entry) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Ok(false),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };

        let record_path = entry.join("record.json");
        let Ok(record_metadata) = std::fs::symlink_metadata(&record_path) else {
            return Ok(false);
        };
        if !record_metadata.is_file() || record_metadata.file_type().is_symlink() {
            return Ok(false);
        }
        let record: GraphRecord = match std::fs::read(record_path)
            .ok()
            .and_then(|contents| serde_json::from_slice::<GraphRecord>(&contents).ok())
        {
            Some(record) if record.format == CACHE_FORMAT => record,
            _ => return Ok(false),
        };
        if record.outputs.len() != self.outputs.len()
            || record.outputs.iter().zip(&self.outputs).enumerate().any(
                |(index, (cached, output))| {
                    cached.label != output.label || cached.file != format!("outputs/{index}")
                },
            )
        {
            return Ok(false);
        }

        for cached in &record.outputs {
            if !is_sha256_hex(&cached.digest) {
                return Ok(false);
            }
            let path = entry.join(&cached.file);
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                return Ok(false);
            };
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() != cached.size
            {
                return Ok(false);
            }
        }
        for (cached, output) in record.outputs.iter().zip(&self.outputs) {
            if !materialize(
                &entry.join(&cached.file),
                &output.path,
                &cached.digest,
                cached.size,
            )? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(crate) fn publish(&self) -> anyhow::Result<()> {
        let entry = self.entry_path();
        match std::fs::symlink_metadata(&entry) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                std::fs::remove_dir_all(&entry)?;
            }
            Ok(_) => bail!(
                "refusing to replace non-directory dependency graph cache entry `{}`",
                entry.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let parent = entry
            .parent()
            .context("dependency graph cache entry has no parent")?;
        std::fs::create_dir_all(parent)?;
        let staging = tempfile::TempDir::new_in(parent)?;
        let outputs_dir = staging.path().join("outputs");
        std::fs::create_dir_all(&outputs_dir)?;

        let mut cached_outputs = Vec::with_capacity(self.outputs.len());
        for (index, output) in self.outputs.iter().enumerate() {
            let metadata = std::fs::metadata(&output.path).with_context(|| {
                format!(
                    "dependency graph did not produce `{}` for `{}`",
                    output.path.display(),
                    output.label
                )
            })?;
            if !metadata.is_file() {
                bail!(
                    "dependency graph produced a non-file output `{}`",
                    output.path.display()
                );
            }
            let file = format!("outputs/{index}");
            let destination = staging.path().join(&file);
            std::fs::copy(&output.path, &destination)?;
            let digest = hex_digest(file_digest(&destination)?);
            let size = std::fs::metadata(&destination)?.len();
            cached_outputs.push(CachedOutput {
                label: output.label.clone(),
                file,
                digest,
                size,
            });
        }

        let record = GraphRecord {
            format: CACHE_FORMAT.to_owned(),
            outputs: cached_outputs,
        };
        let mut record_file = std::fs::File::create(staging.path().join("record.json"))?;
        record_file.write_all(&serde_json::to_vec(&record)?)?;
        record_file.sync_all()?;
        drop(record_file);
        std::fs::rename(staging.path(), entry)?;
        Ok(())
    }

    pub(crate) fn output_file_ids(&self, graph: &Graph) -> Vec<FileId> {
        self.build_ids
            .iter()
            .flat_map(|&build_id| graph.builds[build_id].outs().iter().copied())
            .collect()
    }

    pub(crate) fn detach_builds(&self, graph: &mut Graph) {
        let outputs = self.output_file_ids(graph);
        for output in outputs {
            graph.files.by_id[output].input = None;
        }
    }

    fn entry_path(&self) -> PathBuf {
        sharded_path(
            &self.root.join("graphs/script-dependencies"),
            &self.graph_id,
        )
    }
}

fn dependency_graph_id(actions: &[DependencyBuildAction]) -> anyhow::Result<String> {
    let mut actions = actions.iter().collect::<Vec<_>>();
    actions.sort_by(|left, right| {
        left.description
            .package
            .cmp(&right.description.package)
            .then_with(|| {
                left.description
                    .canonical_args
                    .cmp(&right.description.canonical_args)
            })
    });

    let mut digests = HashMap::new();
    let mut hash = Sha256::new();
    hash_field(&mut hash, b"format");
    hash_field(&mut hash, CACHE_FORMAT.as_bytes());
    hash_field(&mut hash, b"actions");
    hash_field(&mut hash, &(actions.len() as u64).to_le_bytes());
    for action in actions {
        let description = &action.description;
        hash_field(&mut hash, b"package");
        hash_field(&mut hash, description.package.as_bytes());

        hash_field(&mut hash, b"resolution");
        hash_field(
            &mut hash,
            &(description.resolution.len() as u64).to_le_bytes(),
        );
        for fact in &description.resolution {
            hash_field(&mut hash, fact.as_bytes());
        }

        hash_field(&mut hash, b"arguments");
        hash_field(
            &mut hash,
            &(description.canonical_args.len() as u64).to_le_bytes(),
        );
        for argument in &description.canonical_args {
            hash_field(&mut hash, argument.as_bytes());
        }

        let mut inputs = description.inputs.iter().collect::<Vec<_>>();
        inputs.sort_by(|left, right| left.label.cmp(&right.label));
        hash_field(&mut hash, b"inputs");
        hash_field(&mut hash, &(inputs.len() as u64).to_le_bytes());
        for input in inputs {
            hash_field(&mut hash, input.label.as_bytes());
            if input.hash_content {
                let digest = match digests.get(&input.path) {
                    Some(digest) => *digest,
                    None => {
                        let digest = file_digest(&input.path)?;
                        digests.insert(input.path.clone(), digest);
                        digest
                    }
                };
                hash_field(&mut hash, &digest);
            } else {
                hash_field(&mut hash, b"logical-input");
            }
        }

        let mut outputs = description.outputs.iter().collect::<Vec<_>>();
        outputs.sort_by(|left, right| left.label.cmp(&right.label));
        hash_field(&mut hash, b"outputs");
        hash_field(&mut hash, &(outputs.len() as u64).to_le_bytes());
        for output in outputs {
            hash_field(&mut hash, output.label.as_bytes());
        }
    }
    Ok(hex_digest(hash.finalize()))
}

fn materialize(
    source: &Path,
    destination: &Path,
    expected_digest: &str,
    expected_size: u64,
) -> anyhow::Result<bool> {
    let parent = destination
        .parent()
        .context("dependency build output has no parent")?;
    std::fs::create_dir_all(parent)?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?.into_temp_path();
    std::fs::copy(source, &temporary)?;
    let valid = std::fs::metadata(&temporary)?.len() == expected_size
        && hex_digest(file_digest(&temporary)?) == expected_digest;
    if !valid {
        return Ok(false);
    }
    replace_file(&temporary, destination)?;
    Ok(true)
}

fn replace_file(source: &Path, destination: &Path) -> anyhow::Result<()> {
    match std::fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(error)
            if destination.exists()
                && matches!(
                    error.kind(),
                    std::io::ErrorKind::AlreadyExists | std::io::ErrorKind::PermissionDenied
                ) =>
        {
            std::fs::remove_file(destination)?;
            std::fs::rename(source, destination)?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn file_digest(path: &Path) -> anyhow::Result<[u8; 32]> {
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("failed to hash build input `{}`", path.display()))?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash.finalize().into())
}

fn hash_field(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    use std::fmt::Write as _;

    bytes
        .as_ref()
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            write!(hex, "{byte:02x}").expect("writing to a String cannot fail");
            hex
        })
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sharded_path(root: &Path, digest: &str) -> PathBuf {
    let (prefix, rest) = digest.split_at(2);
    root.join(prefix).join(rest)
}
