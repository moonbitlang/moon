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

//! Content-addressed storage for complete lowered-action output sets.
//!
//! The store knows only an action digest and its concrete output paths. It does
//! not decide which actions exist, interpret producer edges, or construct an
//! execution graph.

use std::{
    fs::File,
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

const FORMAT_VERSION: u32 = 1;
const OUTPUT_MANIFEST: &str = "manifest.json";
const PUBLISH_ATTEMPTS: usize = 8;

#[derive(Clone)]
pub(super) struct CacheAction {
    key: String,
    outputs: Vec<PathBuf>,
}

impl CacheAction {
    pub(super) fn new(key: String, mut outputs: Vec<PathBuf>) -> Option<Self> {
        outputs.sort();
        outputs.dedup();
        (!outputs.is_empty()).then_some(Self { key, outputs })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RestoreOutcome {
    Hit,
    Miss,
}

#[derive(Clone)]
pub(super) struct ActionCache {
    root: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct ActionEntry {
    version: u32,
    action_key: String,
    output_id: String,
}

#[derive(Serialize, Deserialize)]
struct OutputManifest {
    version: u32,
    files: Vec<OutputFile>,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
struct OutputFile {
    size: u64,
    digest: String,
}

impl ActionCache {
    pub(super) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn action_entry_path(&self, action: &CacheAction) -> PathBuf {
        self.root
            .join("actions")
            .join(format!("{}.json", action.key))
    }

    pub(super) fn restore(&self, action: &CacheAction) -> anyhow::Result<RestoreOutcome> {
        let entry_path = self.action_entry_path(action);
        let contents = match std::fs::read(&entry_path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(RestoreOutcome::Miss);
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to read {}", entry_path.display()));
            }
        };
        let Ok(entry) = serde_json::from_slice::<ActionEntry>(&contents) else {
            return Ok(RestoreOutcome::Miss);
        };
        if entry.version != FORMAT_VERSION
            || entry.action_key != action.key
            || !is_blake3_hex(&entry.output_id)
        {
            return Ok(RestoreOutcome::Miss);
        }

        let object_dir = self.root.join("objects").join(&entry.output_id);
        let Some(manifest) = read_output_manifest(&object_dir, &entry.output_id)? else {
            return Ok(RestoreOutcome::Miss);
        };
        if manifest.files.len() != action.outputs.len()
            || !object_files_match(&object_dir, &manifest.files)?
        {
            return Ok(RestoreOutcome::Miss);
        }

        let mut staged = Vec::new();
        for (index, (destination, expected)) in
            action.outputs.iter().zip(&manifest.files).enumerate()
        {
            if file_matches(destination, expected)? {
                continue;
            }
            let parent = destination.parent().with_context(|| {
                format!("cached output has no parent: {}", destination.display())
            })?;
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create cached output directory {}",
                    parent.display()
                )
            })?;
            let staged_file = tempfile::NamedTempFile::new_in(parent).with_context(|| {
                format!("failed to stage cached output {}", destination.display())
            })?;
            std::fs::copy(object_dir.join(index.to_string()), staged_file.path()).with_context(
                || format!("failed to copy cached output {}", destination.display()),
            )?;
            staged.push((staged_file, destination, expected));
        }

        for (mut staged_file, destination, expected) in staged {
            let parent = destination.parent().expect("staged outputs have parents");
            let mut last_error = None;
            let mut materialized = false;
            for _ in 0..PUBLISH_ATTEMPTS {
                match staged_file.persist(destination) {
                    Ok(_) => {
                        materialized = true;
                        break;
                    }
                    Err(error) => {
                        last_error = Some(error.error);
                        staged_file = error.file;
                        if file_matches(destination, expected)? {
                            materialized = true;
                            break;
                        }
                        if let Err(error) = quarantine_path(destination, parent) {
                            last_error = Some(error);
                        }
                    }
                }
            }
            if !materialized {
                return Err(last_error.expect("materialization attempted at least once"))
                    .with_context(|| {
                        format!(
                            "failed to materialize cached output {}",
                            destination.display()
                        )
                    });
            }
        }

        for (destination, expected) in action.outputs.iter().zip(&manifest.files) {
            if !file_matches(destination, expected)? {
                return Ok(RestoreOutcome::Miss);
            }
        }
        Ok(RestoreOutcome::Hit)
    }

    pub(super) fn publish(&self, action: &CacheAction) -> anyhow::Result<()> {
        let mut files = Vec::with_capacity(action.outputs.len());
        for path in &action.outputs {
            let metadata = std::fs::metadata(path)
                .with_context(|| format!("action did not produce {}", path.display()))?;
            if !metadata.is_file() {
                // The first cache slice stores regular compiler artifacts.
                // Directory outputs continue through the miss executor.
                return Ok(());
            }
            files.push(OutputFile {
                size: metadata.len(),
                digest: digest_file(path)?.to_hex().to_string(),
            });
        }

        let manifest = OutputManifest {
            version: FORMAT_VERSION,
            files,
        };
        let manifest_contents =
            serde_json::to_vec(&manifest).context("failed to serialize output manifest")?;
        let output_id = blake3::hash(&manifest_contents).to_hex().to_string();

        self.publish_output_object(action, &output_id, &manifest, &manifest_contents)?;
        self.publish_action_entry(action, &output_id)
    }

    fn publish_output_object(
        &self,
        action: &CacheAction,
        output_id: &str,
        manifest: &OutputManifest,
        manifest_contents: &[u8],
    ) -> anyhow::Result<()> {
        let objects_root = self.root.join("objects");
        std::fs::create_dir_all(&objects_root).with_context(|| {
            format!(
                "failed to create cache object directory {}",
                objects_root.display()
            )
        })?;
        let object_dir = objects_root.join(output_id);
        if output_object_matches(&object_dir, output_id, &manifest.files)? {
            return Ok(());
        }

        let staging = tempfile::Builder::new()
            .prefix(".staging-")
            .tempdir_in(&objects_root)
            .context("failed to stage cache object")?;
        for (index, output) in action.outputs.iter().enumerate() {
            std::fs::copy(output, staging.path().join(index.to_string()))
                .with_context(|| format!("failed to stage output {}", output.display()))?;
        }
        std::fs::write(staging.path().join(OUTPUT_MANIFEST), manifest_contents)
            .context("failed to stage output manifest")?;
        if !output_object_matches(staging.path(), output_id, &manifest.files)? {
            bail!("action outputs changed while publishing cache object {output_id}");
        }

        let mut last_error = None;
        for _ in 0..PUBLISH_ATTEMPTS {
            match std::fs::rename(staging.path(), &object_dir) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    last_error = Some(error);
                    if output_object_matches(&object_dir, output_id, &manifest.files)? {
                        return Ok(());
                    }
                    if let Err(error) = quarantine_path(&object_dir, &objects_root) {
                        last_error = Some(error);
                    }
                }
            }
        }
        Err(last_error.expect("publishing attempted at least once"))
            .with_context(|| format!("failed to publish cache object {}", object_dir.display()))
    }

    fn publish_action_entry(&self, action: &CacheAction, output_id: &str) -> anyhow::Result<()> {
        let actions_root = self.root.join("actions");
        std::fs::create_dir_all(&actions_root).with_context(|| {
            format!(
                "failed to create cache action directory {}",
                actions_root.display()
            )
        })?;
        let entry_path = self.action_entry_path(action);
        let entry = ActionEntry {
            version: FORMAT_VERSION,
            action_key: action.key.clone(),
            output_id: output_id.to_owned(),
        };

        let mut last_error = None;
        for _ in 0..PUBLISH_ATTEMPTS {
            let mut staged_entry = tempfile::NamedTempFile::new_in(&actions_root)
                .context("failed to stage cache action entry")?;
            serde_json::to_writer(&mut staged_entry, &entry)
                .context("failed to serialize cache action entry")?;
            staged_entry
                .flush()
                .context("failed to flush cache action entry")?;
            match staged_entry.persist(&entry_path) {
                Ok(_) => return Ok(()),
                Err(error) => {
                    last_error = Some(error.error);
                    if self.action_entry_is_usable(&entry_path, action)? {
                        return Ok(());
                    }
                    if let Err(error) = quarantine_path(&entry_path, &actions_root) {
                        last_error = Some(error);
                    }
                }
            }
        }
        Err(last_error.expect("publishing attempted at least once")).with_context(|| {
            format!(
                "failed to publish cache action entry {}",
                entry_path.display()
            )
        })
    }

    fn action_entry_is_usable(
        &self,
        entry_path: &Path,
        action: &CacheAction,
    ) -> anyhow::Result<bool> {
        let contents = match std::fs::read(entry_path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        let Ok(entry) = serde_json::from_slice::<ActionEntry>(&contents) else {
            return Ok(false);
        };
        if entry.version != FORMAT_VERSION
            || entry.action_key != action.key
            || !is_blake3_hex(&entry.output_id)
        {
            return Ok(false);
        }
        let object_dir = self.root.join("objects").join(&entry.output_id);
        let Some(manifest) = read_output_manifest(&object_dir, &entry.output_id)? else {
            return Ok(false);
        };
        Ok(manifest.files.len() == action.outputs.len()
            && object_files_match(&object_dir, &manifest.files)?)
    }
}

fn quarantine_path(path: &Path, parent: &Path) -> std::io::Result<()> {
    let quarantine = tempfile::Builder::new()
        .prefix(".replaced-")
        .tempdir_in(parent)?;
    match std::fs::rename(path, quarantine.path().join("entry")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn read_output_manifest(
    object_dir: &Path,
    output_id: &str,
) -> anyhow::Result<Option<OutputManifest>> {
    let manifest_path = object_dir.join(OUTPUT_MANIFEST);
    let contents = match std::fs::read(&manifest_path) {
        Ok(contents) => contents,
        Err(error) if is_missing_or_wrong_type(error.kind()) => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("failed to read output manifest {}", manifest_path.display())
            });
        }
    };
    if blake3::hash(&contents).to_hex().as_str() != output_id {
        return Ok(None);
    }
    let Ok(manifest) = serde_json::from_slice::<OutputManifest>(&contents) else {
        return Ok(None);
    };
    Ok((manifest.version == FORMAT_VERSION
        && manifest
            .files
            .iter()
            .all(|file| is_blake3_hex(&file.digest)))
    .then_some(manifest))
}

fn output_object_matches(
    object_dir: &Path,
    output_id: &str,
    expected: &[OutputFile],
) -> anyhow::Result<bool> {
    let Some(manifest) = read_output_manifest(object_dir, output_id)? else {
        return Ok(false);
    };
    Ok(manifest.files == expected && object_files_match(object_dir, &manifest.files)?)
}

fn object_files_match(object_dir: &Path, files: &[OutputFile]) -> anyhow::Result<bool> {
    for (index, expected) in files.iter().enumerate() {
        if !file_matches(&object_dir.join(index.to_string()), expected)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn is_blake3_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn file_matches(path: &Path, expected: &OutputFile) -> anyhow::Result<bool> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if is_missing_or_wrong_type(error.kind()) => return Ok(false),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect cached output {}", path.display()));
        }
    };
    if !metadata.is_file() || metadata.len() != expected.size {
        return Ok(false);
    }
    match digest_file(path) {
        Ok(digest) => Ok(digest.to_hex().as_str() == expected.digest),
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| is_missing_or_wrong_type(error.kind())) =>
        {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

fn digest_file(path: &Path) -> anyhow::Result<blake3::Hash> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize())
}

fn is_missing_or_wrong_type(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::NotFound
            | ErrorKind::NotADirectory
            | ErrorKind::IsADirectory
            | ErrorKind::InvalidInput
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use super::*;

    fn cached_action(root: &Path, outputs: &[(&str, &str)]) -> (ActionCache, CacheAction) {
        let store = ActionCache::new(root.join("cache"));
        let output_paths = outputs
            .iter()
            .map(|(name, content)| {
                let path = root.join("build").join(name);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, content).unwrap();
                path
            })
            .collect();
        (
            store,
            CacheAction::new(blake3::hash(b"action").to_hex().to_string(), output_paths).unwrap(),
        )
    }

    fn action_entry(store: &ActionCache, action: &CacheAction) -> ActionEntry {
        serde_json::from_slice(&std::fs::read(store.action_entry_path(action)).unwrap()).unwrap()
    }

    #[test]
    fn corrupted_manifest_is_a_miss() {
        let root = tempfile::tempdir().unwrap();
        let (store, action) = cached_action(root.path(), &[("dependency.mi", "interface")]);
        store.publish(&action).unwrap();
        let entry = action_entry(&store, &action);
        std::fs::write(
            store
                .root
                .join("objects")
                .join(entry.output_id)
                .join(OUTPUT_MANIFEST),
            b"corrupt",
        )
        .unwrap();

        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Miss);
    }

    #[test]
    fn damaged_missing_and_partial_objects_are_safe_misses() {
        let root = tempfile::tempdir().unwrap();
        let (store, action) = cached_action(
            root.path(),
            &[("dependency.mi", "interface"), ("dependency.core", "core")],
        );
        store.publish(&action).unwrap();
        let entry = action_entry(&store, &action);
        let object_dir = store.root.join("objects").join(entry.output_id);

        std::fs::write(object_dir.join("1"), "bad!").unwrap();
        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Miss);
        store
            .publish(&action)
            .expect("publishing should repair a damaged object");
        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Hit);

        std::fs::remove_file(object_dir.join("1")).unwrap();
        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Miss);

        std::fs::remove_dir_all(object_dir).unwrap();
        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Miss);
    }

    #[test]
    fn malformed_and_old_action_entries_are_misses() {
        let root = tempfile::tempdir().unwrap();
        let (store, action) = cached_action(root.path(), &[("dependency.mi", "interface")]);
        let entry_path = store.action_entry_path(&action);
        std::fs::create_dir_all(entry_path.parent().unwrap()).unwrap();
        std::fs::write(&entry_path, b"not-json").unwrap();
        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Miss);

        std::fs::write(
            &entry_path,
            serde_json::to_vec(&ActionEntry {
                version: FORMAT_VERSION - 1,
                action_key: action.key.clone(),
                output_id: blake3::hash(b"old").to_hex().to_string(),
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Miss);

        std::fs::write(
            &entry_path,
            serde_json::to_vec(&ActionEntry {
                version: FORMAT_VERSION,
                action_key: blake3::hash(b"different-action").to_hex().to_string(),
                output_id: blake3::hash(b"old").to_hex().to_string(),
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Miss);
    }

    #[test]
    fn duplicate_publish_restores_complete_outputs() {
        let root = tempfile::tempdir().unwrap();
        let (store, action) = cached_action(
            root.path(),
            &[("dependency.mi", "interface"), ("dependency.core", "core")],
        );
        store.publish(&action).unwrap();
        store.publish(&action).unwrap();
        for output in &action.outputs {
            std::fs::remove_file(output).unwrap();
        }

        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Hit);
        assert_eq!(std::fs::read_to_string(&action.outputs[0]).unwrap(), "core");
        assert_eq!(
            std::fs::read_to_string(&action.outputs[1]).unwrap(),
            "interface"
        );
    }

    #[test]
    fn restore_replaces_stale_outputs() {
        let root = tempfile::tempdir().unwrap();
        let (store, action) = cached_action(root.path(), &[("dependency.mi", "interface")]);
        store.publish(&action).unwrap();
        std::fs::write(&action.outputs[0], "stale").unwrap();

        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Hit);
        assert_eq!(
            std::fs::read_to_string(&action.outputs[0]).unwrap(),
            "interface"
        );
    }

    #[test]
    fn concurrent_publish_loser_validates_winner() {
        let root = tempfile::tempdir().unwrap();
        let (store, action) = cached_action(root.path(), &[("dependency.mi", "interface")]);
        let store = Arc::new(store);
        let action = Arc::new(action);
        let barrier = Arc::new(Barrier::new(8));
        let writers = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                let action = Arc::clone(&action);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    store.publish(&action)
                })
            })
            .collect::<Vec<_>>();
        for writer in writers {
            writer.join().unwrap().unwrap();
        }
        std::fs::remove_file(&action.outputs[0]).unwrap();
        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Hit);
    }

    #[test]
    fn concurrent_restore_and_publish_remain_successful() {
        let root = tempfile::tempdir().unwrap();
        let (store, action) = cached_action(root.path(), &[("dependency.mi", "interface")]);
        store.publish(&action).unwrap();
        let store = Arc::new(store);
        let action = Arc::new(action);
        let barrier = Arc::new(Barrier::new(3));

        let publisher = {
            let store = Arc::clone(&store);
            let action = Arc::clone(&action);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                for _ in 0..20 {
                    store.publish(&action)?;
                }
                anyhow::Ok(())
            })
        };
        let restorer = {
            let store = Arc::clone(&store);
            let action = Arc::clone(&action);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                for _ in 0..20 {
                    assert_eq!(store.restore(&action)?, RestoreOutcome::Hit);
                }
                anyhow::Ok(())
            })
        };
        barrier.wait();
        publisher.join().unwrap().unwrap();
        restorer.join().unwrap().unwrap();
    }

    #[test]
    fn concurrent_restores_materialize_the_same_output() {
        let root = tempfile::tempdir().unwrap();
        let (store, action) = cached_action(root.path(), &[("dependency.mi", "interface")]);
        store.publish(&action).unwrap();
        std::fs::remove_file(&action.outputs[0]).unwrap();

        let store = Arc::new(store);
        let action = Arc::new(action);
        let barrier = Arc::new(Barrier::new(8));
        let restorers = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                let action = Arc::clone(&action);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    store.restore(&action)
                })
            })
            .collect::<Vec<_>>();
        for restorer in restorers {
            assert_eq!(restorer.join().unwrap().unwrap(), RestoreOutcome::Hit);
        }
        assert_eq!(
            std::fs::read_to_string(&action.outputs[0]).unwrap(),
            "interface"
        );
    }
}
