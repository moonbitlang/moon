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

//! Publish build metadata consumed by the compiler and developer tools.
//!
//! RR constructs the package documents; this module owns serialization and
//! filesystem publication. `BuildSession` calls it under the target-directory
//! lock before execution and skips publication for dry runs.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::Path,
};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tracing::{Level, instrument};

use crate::{BuildMeta, execution::BuildInput};

#[derive(Debug, Serialize, Deserialize)]
struct PackagesSelectorJSON {
    backend: String,
    opt_level: String,
}

/// Generate the backend/profile/run-mode-scoped `packages.json` document.
///
/// `build_meta` and the check commands must describe the same invocation.
/// This writes only the scoped document, leaving the universal selector and
/// index untouched.
#[instrument(level = Level::DEBUG, skip_all)]
pub(super) fn generate_metadata(
    source_dir: &Path,
    build_meta: &BuildMeta,
    check_commands: &moonbuild_rupes_recta::metadata::CheckCommandMap<'_>,
) -> anyhow::Result<()> {
    let layout = build_meta.artifact_paths.target_layout();
    let scoped_metadata_file = layout.packages_json_path();

    let metadata = moonbuild_rupes_recta::metadata::gen_metadata_json(
        &build_meta.resolved_project,
        source_dir,
        &build_meta.artifact_paths,
        build_meta.configuration(),
        check_commands,
    );
    let meta = serde_json::to_string_pretty(&metadata).context("Failed to serialize metadata")?;
    write_metadata_if_changed(&scoped_metadata_file, &meta)
}

/// Publish metadata for one complete Check invocation.
///
/// All plans must share a target directory. Publish each scoped document before
/// the active selector, then publish the backend index last so readers never
/// discover a new index before its documents and selector are ready.
/// An empty invocation leaves existing metadata untouched.
#[instrument(level = Level::DEBUG, skip_all)]
pub(super) fn publish_check_metadata(
    source_dir: &Path,
    build_metas: &[&BuildMeta],
    check_commands: &moonbuild_rupes_recta::metadata::CheckCommandMap<'_>,
) -> anyhow::Result<()> {
    let Some(first) = build_metas.first() else {
        return Ok(());
    };
    for build_meta in build_metas {
        generate_all_pkgs_json(build_meta)?;
        generate_metadata(source_dir, build_meta, check_commands)?;
    }

    // The compiler's universal format selects one active projection.
    // Before the split, repeated publication made the final planned
    // backend authoritative, so retain that deterministic behavior.
    let selected = build_metas.last().expect("non-empty planned runs");
    let selector = PackagesSelectorJSON {
        backend: selected.target_backend().to_string(),
        opt_level: selected.configuration().profile.as_str().to_string(),
    };
    let selector = serde_json::to_string_pretty(&selector)
        .context("Failed to serialize universal packages metadata")?;
    let metadata_file = selected
        .artifact_paths
        .target_layout()
        .packages_selector_path();
    write_metadata_if_changed(&metadata_file, &selector)?;

    let metadata_file = first.artifact_paths.target_layout().packages_index_path();
    let backends = build_metas
        .iter()
        .map(|build_meta| build_meta.target_backend())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|backend| backend.to_string())
        .collect::<Vec<_>>();
    let index = serde_json::to_string_pretty(&backends)
        .context("Failed to serialize packages metadata index")?;
    write_metadata_if_changed(&metadata_file, &index)
}

fn write_metadata_if_changed(metadata_file: &Path, metadata: &str) -> anyhow::Result<()> {
    let orig_meta = std::fs::read_to_string(metadata_file);
    if !orig_meta.is_ok_and(|original| original == metadata) {
        let parent = metadata_file
            .parent()
            .context("build metadata path must have a parent directory")?;
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create directory for build metadata at {}",
                parent.display()
            )
        })?;
        let mut staged = tempfile::NamedTempFile::new_in(parent).with_context(|| {
            format!(
                "Failed to stage build metadata for {}",
                metadata_file.display()
            )
        })?;
        staged.write_all(metadata.as_bytes()).with_context(|| {
            format!(
                "Failed to write staged build metadata for {}",
                metadata_file.display()
            )
        })?;
        staged
            .persist(metadata_file)
            .map_err(|error| error.error)
            .with_context(|| {
                format!(
                    "Failed to publish build metadata to {}",
                    metadata_file.display()
                )
            })?;
    }
    Ok(())
}

pub(super) fn collect_check_commands_by_output(
    build_input: &BuildInput,
) -> moonbuild_rupes_recta::metadata::CheckCommandMap<'_> {
    let mut commands = BTreeMap::new();
    for id in build_input.execution_plan().action_ids() {
        let action = build_input.execution_plan().action(id);
        let Some(command_args) = check_command_args_without_executable(action.command().args())
        else {
            continue;
        };
        for output in action.outputs() {
            commands.insert(output.clone(), command_args);
        }
    }
    commands
}

fn check_command_args_without_executable(args: &[String]) -> Option<&[String]> {
    let (_executable, command_args) = args.split_first()?;
    command_args
        .first()
        .is_some_and(|arg| arg == "check")
        .then_some(command_args)
}

/// Write the artifact paths needed by the compiler for indirect dependencies.
pub(super) fn generate_all_pkgs_json(build_meta: &BuildMeta) -> anyhow::Result<()> {
    let all_pkgs_path = build_meta
        .artifact_paths
        .target_layout()
        .all_pkgs_of_build_target();
    let all_pkgs_json = moonbuild_rupes_recta::all_pkgs::gen_all_pkgs_json(
        &build_meta.resolved_project,
        &build_meta.artifact_paths,
        build_meta.target_backend(),
    );
    let orig_all_pkgs = std::fs::read_to_string(&all_pkgs_path);
    let all_pkgs_str =
        serde_json::to_string_pretty(&all_pkgs_json).context("Failed to serialize metadata")?;

    // Only overwrite if changed
    if !orig_all_pkgs.is_ok_and(|o| o == all_pkgs_str) {
        // Ensure parent directory exists
        if let Some(parent) = all_pkgs_path.parent() {
            std::fs::create_dir_all(parent).context(format!(
                "Failed to create directory for all_pkgs at {}",
                parent.display()
            ))?;
        }
        std::fs::write(&all_pkgs_path, all_pkgs_str).context(format!(
            "Failed to write all_pkgs to the path {}",
            all_pkgs_path.display()
        ))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::PackagesSelectorJSON;

    #[test]
    fn packages_selector_json_matches_compiler_contract() {
        let value = serde_json_lenient::to_string(&PackagesSelectorJSON {
            backend: "wasm-gc".to_string(),
            opt_level: "debug".to_string(),
        })
        .unwrap();
        assert_eq!(value, r#"{"backend":"wasm-gc","opt_level":"debug"}"#);
    }

    #[cfg(unix)]
    #[test]
    fn metadata_write_preserves_equal_files_and_replaces_changed_files() {
        use super::write_metadata_if_changed;
        use std::os::unix::fs::MetadataExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("metadata");

        write_metadata_if_changed(&path, "first").unwrap();
        let first_inode = path.metadata().unwrap().ino();

        write_metadata_if_changed(&path, "first").unwrap();
        assert_eq!(path.metadata().unwrap().ino(), first_inode);

        write_metadata_if_changed(&path, "second").unwrap();
        assert_ne!(path.metadata().unwrap().ino(), first_inode);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "second");
    }
}
