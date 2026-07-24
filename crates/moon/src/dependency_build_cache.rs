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
//! module restores the complete reusable output set into the invocation's
//! target directory, including native dependency C objects and archives, after
//! which those producer nodes are detached from the project-local n2 graph.

use std::{
    collections::HashMap,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use moonbuild_rupes_recta::dependency_build_cache::{DependencyBuildAction, InputIdentity};
use moonutil::{
    cache::{CacheKind, CacheRoot, initialize_cache_root, resolve_cache_root},
    locks::FileLock,
};
use n2::graph::{FileId, Graph};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CACHE_FORMAT: &str = "moon-script-dependency-graph-v2";
const FILE_DIGEST_FORMAT: &str = "moon-file-content-sha256-v1";
// Small inputs are cheaper to hash again than to maintain individual metadata
// records. This threshold primarily captures compiler/tool binaries.
const PERSISTENT_DIGEST_MIN_SIZE: u64 = 1024 * 1024;

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

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct FileFingerprint {
    path: String,
    size: u64,
    modified_nanos: Option<u128>,
    created_nanos: Option<u128>,
    platform_identity: Vec<i128>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileDigestRecord {
    format: String,
    fingerprint: FileFingerprint,
    digest: String,
}

struct FileDigestCache {
    root: PathBuf,
}

impl DependencyGraphCache {
    #[tracing::instrument(level = "debug", skip_all, fields(actions = actions.len()))]
    pub(crate) fn open(actions: &[DependencyBuildAction]) -> anyhow::Result<Option<Self>> {
        if actions.is_empty() {
            return Ok(None);
        }
        let CacheRoot::Path(root) = resolve_cache_root(CacheKind::BuildArtifacts)? else {
            return Ok(None);
        };
        initialize_cache_root(CacheKind::BuildArtifacts, &root)?;

        let graph_id = dependency_graph_id(actions, &root)?;
        let mut descriptions = actions.iter().collect::<Vec<_>>();
        descriptions.sort_by(|left, right| {
            left.description
                .kind
                .cmp(&right.description.kind)
                .then_with(|| left.description.package.cmp(&right.description.package))
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

    #[tracing::instrument(level = "debug", skip_all, fields(graph_id = %self.graph_id))]
    pub(crate) fn lock(&self) -> anyhow::Result<FileLock> {
        let directory = sharded_path(&self.root.join("locks/dependency-graphs"), &self.graph_id);
        std::fs::create_dir_all(&directory)?;
        Ok(FileLock::lock_with_verbosity(&directory, false)?)
    }

    #[tracing::instrument(level = "debug", skip_all, fields(graph_id = %self.graph_id))]
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

    #[tracing::instrument(level = "debug", skip_all, fields(graph_id = %self.graph_id))]
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

#[tracing::instrument(level = "debug", skip_all, fields(actions = actions.len()))]
fn dependency_graph_id(actions: &[DependencyBuildAction], root: &Path) -> anyhow::Result<String> {
    let mut actions = actions.iter().collect::<Vec<_>>();
    actions.sort_by(|left, right| {
        left.description
            .kind
            .cmp(&right.description.kind)
            .then_with(|| left.description.package.cmp(&right.description.package))
            .then_with(|| {
                left.description
                    .canonical_args
                    .cmp(&right.description.canonical_args)
            })
    });

    let digest_cache = FileDigestCache {
        root: root.join("metadata/file-digests"),
    };
    let mut digests = HashMap::new();
    let mut hash = Sha256::new();
    hash_field(&mut hash, b"format");
    hash_field(&mut hash, CACHE_FORMAT.as_bytes());
    hash_field(&mut hash, b"actions");
    hash_field(&mut hash, &(actions.len() as u64).to_le_bytes());
    for action in actions {
        let description = &action.description;
        hash_field(&mut hash, b"kind");
        hash_field(
            &mut hash,
            match description.kind {
                moonbuild_rupes_recta::dependency_build_cache::DependencyBuildKind::MooncBuildCore => {
                    b"moonc-build-core"
                }
                moonbuild_rupes_recta::dependency_build_cache::DependencyBuildKind::CStubObject => {
                    b"c-stub-object"
                }
                moonbuild_rupes_recta::dependency_build_cache::DependencyBuildKind::CStubLibrary => {
                    b"c-stub-library"
                }
                moonbuild_rupes_recta::dependency_build_cache::DependencyBuildKind::NativeRuntime => {
                    b"native-runtime"
                }
            },
        );
        hash_field(&mut hash, b"package");
        hash_field(&mut hash, description.package.as_bytes());

        hash_field(&mut hash, b"environment");
        hash_field(
            &mut hash,
            &(description.environment.len() as u64).to_le_bytes(),
        );
        for (name, value) in &description.environment {
            hash_field(&mut hash, name.as_bytes());
            hash_field(&mut hash, value.as_bytes());
        }

        hash_field(&mut hash, b"resolution");
        hash_field(
            &mut hash,
            &(description.resolution.len() as u64).to_le_bytes(),
        );
        for fact in &description.resolution {
            hash_field(&mut hash, fact.module.as_bytes());
            hash_field(&mut hash, fact.version.as_bytes());
            match &fact.source_checksum {
                Some(checksum) => {
                    hash_field(&mut hash, b"prepared-source");
                    hash_field(&mut hash, checksum.as_bytes());
                }
                None => hash_field(&mut hash, b"other-source"),
            }
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
            match input.identity {
                InputIdentity::Content | InputIdentity::Tool => {
                    let digest = match digests.get(&input.path) {
                        Some(digest) => *digest,
                        None => {
                            let digest = if input.identity == InputIdentity::Tool {
                                digest_cache.digest_tool(&input.path)?
                            } else {
                                digest_cache.digest(&input.path)?
                            };
                            digests.insert(input.path.clone(), digest);
                            digest
                        }
                    };
                    hash_field(
                        &mut hash,
                        if input.identity == InputIdentity::Tool {
                            b"tool"
                        } else {
                            b"content"
                        },
                    );
                    hash_field(&mut hash, &digest);
                }
                InputIdentity::Logical => hash_field(&mut hash, b"logical"),
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

impl FileDigestCache {
    fn digest_tool(&self, path: &Path) -> anyhow::Result<[u8; 32]> {
        let resolved = if path.is_file() {
            path.to_path_buf()
        } else {
            which::which(path).with_context(|| {
                format!(
                    "failed to resolve build tool `{}` through PATH",
                    path.display()
                )
            })?
        };
        self.digest(&resolved)
    }

    #[tracing::instrument(level = "debug", skip_all, fields(path = %path.display()))]
    fn digest(&self, path: &Path) -> anyhow::Result<[u8; 32]> {
        let metadata = std::fs::metadata(path)
            .with_context(|| format!("failed to inspect build input `{}`", path.display()))?;
        if metadata.len() < PERSISTENT_DIGEST_MIN_SIZE {
            return file_digest(path);
        }
        let fingerprint = file_fingerprint(path, &metadata)?;
        let entry = self.entry_path(&fingerprint)?;
        if let Ok(contents) = std::fs::read(&entry)
            && let Ok(record) = serde_json::from_slice::<FileDigestRecord>(&contents)
            && record.format == FILE_DIGEST_FORMAT
            && record.fingerprint == fingerprint
            && let Some(digest) = parse_hex_digest(&record.digest)
        {
            return Ok(digest);
        }

        let digest = file_digest(path)?;
        // This memo is an optimization only. The graph identity still contains
        // the content digest, so a failed metadata write must not block builds.
        let _ = self.publish(&entry, &fingerprint, digest);
        Ok(digest)
    }

    fn entry_path(&self, fingerprint: &FileFingerprint) -> anyhow::Result<PathBuf> {
        let mut hash = Sha256::new();
        hash_field(&mut hash, FILE_DIGEST_FORMAT.as_bytes());
        hash_field(&mut hash, &serde_json::to_vec(fingerprint)?);
        Ok(sharded_path(&self.root, &hex_digest(hash.finalize())))
    }

    fn publish(
        &self,
        entry: &Path,
        fingerprint: &FileFingerprint,
        digest: [u8; 32],
    ) -> anyhow::Result<()> {
        let parent = entry
            .parent()
            .context("file digest cache entry has no parent")?;
        std::fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer(
            &mut temporary,
            &FileDigestRecord {
                format: FILE_DIGEST_FORMAT.to_owned(),
                fingerprint: FileFingerprint {
                    path: fingerprint.path.clone(),
                    size: fingerprint.size,
                    modified_nanos: fingerprint.modified_nanos,
                    created_nanos: fingerprint.created_nanos,
                    platform_identity: fingerprint.platform_identity.clone(),
                },
                digest: hex_digest(digest),
            },
        )?;
        temporary.flush()?;
        // Digest memos are disposable accelerators. Atomic replacement avoids
        // partial records; forcing each one to stable storage would serialize
        // cold builds behind many unrelated fsyncs.
        replace_file(temporary.into_temp_path().as_ref(), entry)
    }
}

fn file_fingerprint(path: &Path, metadata: &std::fs::Metadata) -> anyhow::Result<FileFingerprint> {
    use std::time::UNIX_EPOCH;

    let system_time_nanos = |time: std::io::Result<std::time::SystemTime>| {
        time.ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos())
    };

    #[cfg(unix)]
    let platform_identity = {
        use std::os::unix::fs::MetadataExt;
        vec![
            i128::from(metadata.dev()),
            i128::from(metadata.ino()),
            i128::from(metadata.mtime()),
            i128::from(metadata.mtime_nsec()),
            i128::from(metadata.ctime()),
            i128::from(metadata.ctime_nsec()),
        ]
    };
    #[cfg(not(unix))]
    let platform_identity = Vec::new();

    Ok(FileFingerprint {
        path: dunce::canonicalize(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .into_owned(),
        size: metadata.len(),
        modified_nanos: system_time_nanos(metadata.modified()),
        created_nanos: system_time_nanos(metadata.created()),
        platform_identity,
    })
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

fn parse_hex_digest(value: &str) -> Option<[u8; 32]> {
    if !is_sha256_hex(value) {
        return None;
    }
    let mut digest = [0_u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(digest)
}

fn sharded_path(root: &Path, digest: &str) -> PathBuf {
    let (prefix, rest) = digest.split_at(2);
    root.join(prefix).join(rest)
}
