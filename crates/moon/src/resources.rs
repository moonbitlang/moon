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

//! Development resource mappings for executable packages.
//!
//! This is a command-level post-build action rather than a compiler output. Callers
//! reconcile mappings only after a successful build and while they still hold
//! the target-directory lock.

use std::path::Path;

use anyhow::Context;
use moonbuild_rupes_recta::model::{BuildPlanNode, TargetKind};
use moonutil::package::validate_data_directory;

pub(crate) fn reconcile_resource_mappings(
    build_meta: &crate::rr_build::BuildMeta,
) -> anyhow::Result<()> {
    for (node, artifacts) in &build_meta.artifacts {
        let BuildPlanNode::MakeExecutable(target) = node else {
            continue;
        };
        if target.kind != TargetKind::Source {
            continue;
        }

        let package = build_meta
            .resolve_output
            .pkg_dirs
            .get_package(target.package);
        if !package.raw.is_main || package.manifest_path.is_none() {
            continue;
        }

        let artifact = artifacts
            .artifacts
            .first()
            .expect("MakeExecutable must produce an executable artifact");
        let artifact_parent = artifact
            .parent()
            .expect("executable artifact must have a parent directory");
        let Some(data_dir) = package.raw.data_dir.as_deref() else {
            continue;
        };

        let source = validate_data_directory(&package.root_path, data_dir)?;
        let destination = artifact_parent.join(data_dir);
        reconcile_resource_mapping_for_platform(&source, &destination)?;
    }
    Ok(())
}

#[cfg(test)]
fn reconcile_resource_mapping(
    package_root: &Path,
    data_dir: &str,
    destination: &Path,
) -> anyhow::Result<()> {
    let source = validate_data_directory(package_root, data_dir)?;
    reconcile_resource_mapping_for_platform(&source, destination)
}

#[cfg(unix)]
fn reconcile_resource_mapping_for_platform(
    source: &Path,
    destination: &Path,
) -> anyhow::Result<()> {
    match std::fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let target = std::fs::read_link(destination)?;
            let target = if target.is_absolute() {
                target
            } else {
                destination
                    .parent()
                    .expect("resource mapping must have a parent directory")
                    .join(target)
            };
            let source = std::fs::canonicalize(source)?;
            if std::fs::canonicalize(target).is_ok_and(|target| target == source) {
                return Ok(());
            }
            std::fs::remove_file(destination).with_context(|| {
                format!(
                    "failed to remove stale executable resource mapping '{}'",
                    destination.display()
                )
            })?;
        }
        Ok(_) => {
            anyhow::bail!(
                "cannot map executable resources from '{}' to '{}': destination already exists",
                source.display(),
                destination.display()
            )
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err).context("failed to inspect executable resource mapping"),
    }

    std::os::unix::fs::symlink(source, destination).with_context(|| {
        format!(
            "failed to map executable resources from '{}' to '{}'",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

#[cfg(windows)]
fn reconcile_resource_mapping_for_platform(
    source: &Path,
    destination: &Path,
) -> anyhow::Result<()> {
    match std::fs::symlink_metadata(destination) {
        Ok(metadata) => match junction::exists(destination) {
            Ok(true) => {
                let source = dunce::canonicalize(source)?;
                if dunce::canonicalize(std::fs::read_link(destination)?)
                    .is_ok_and(|target| target == source)
                {
                    return Ok(());
                }
                remove_resource_junction(destination)?;
            }
            // `junction::exists` follows the target and therefore returns false
            // for a dangling junction. `symlink_metadata` does not follow it,
            // and Rust classifies the junction's name-surrogate mount-point tag
            // as a symlink. `junction::delete` then accepts only that tag, leaving
            // other symbolic links and reparse points untouched.
            Ok(false) if metadata.file_type().is_symlink() => {
                if junction::delete(destination).is_ok() {
                    std::fs::remove_dir(destination).with_context(|| {
                        format!(
                            "failed to remove stale executable resource junction directory '{}'",
                            destination.display()
                        )
                    })?;
                } else {
                    anyhow::bail!(
                        "cannot map executable resources from '{}' to '{}': destination already exists",
                        source.display(),
                        destination.display()
                    );
                }
            }
            Ok(false) => {
                anyhow::bail!(
                    "cannot map executable resources from '{}' to '{}': destination already exists",
                    source.display(),
                    destination.display()
                );
            }
            Err(err) => {
                return Err(err).with_context(|| {
                    format!(
                        "cannot map executable resources from '{}' to '{}': destination already exists",
                        source.display(),
                        destination.display()
                    )
                });
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(err).with_context(|| {
                format!(
                    "failed to inspect executable resource mapping '{}'",
                    destination.display()
                )
            });
        }
    }

    junction::create(source, destination).with_context(|| {
        format!(
            "failed to map executable resources from '{}' to '{}' with an NTFS junction",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

#[cfg(windows)]
fn remove_resource_junction(destination: &Path) -> anyhow::Result<()> {
    junction::delete(destination).with_context(|| {
        format!(
            "failed to remove stale executable resource junction '{}'",
            destination.display()
        )
    })?;
    std::fs::remove_dir(destination).with_context(|| {
        format!(
            "failed to remove stale executable resource junction directory '{}'",
            destination.display()
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_resource_mapping_is_replaced_without_touching_its_old_target() {
        let temp = tempfile::tempdir().unwrap();
        let old_source = temp.path().join("old");
        let new_source = temp.path().join("new");
        let destination = temp.path().join("resources");
        std::fs::create_dir(&old_source).unwrap();
        std::fs::create_dir(&new_source).unwrap();
        std::fs::write(old_source.join("keep.txt"), "keep").unwrap();
        std::fs::write(new_source.join("current.txt"), "current").unwrap();

        reconcile_resource_mapping(temp.path(), "old", &destination).unwrap();
        reconcile_resource_mapping(temp.path(), "new", &destination).unwrap();

        assert_eq!(
            std::fs::read_to_string(destination.join("current.txt")).unwrap(),
            "current"
        );
        assert_eq!(
            std::fs::read_to_string(old_source.join("keep.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn resource_mapping_with_a_missing_target_is_replaced() {
        let temp = tempfile::tempdir().unwrap();
        let old_source = temp.path().join("old");
        let new_source = temp.path().join("new");
        let destination = temp.path().join("resources");
        std::fs::create_dir(&old_source).unwrap();

        reconcile_resource_mapping(temp.path(), "old", &destination).unwrap();
        // On Windows this leaves a dangling junction and exercises the
        // non-following `symlink_metadata` classification used by reconciliation.
        std::fs::remove_dir(&old_source).unwrap();
        std::fs::create_dir(&new_source).unwrap();
        std::fs::write(new_source.join("current.txt"), "current").unwrap();

        reconcile_resource_mapping(temp.path(), "new", &destination).unwrap();

        assert_eq!(
            std::fs::read_to_string(destination.join("current.txt")).unwrap(),
            "current"
        );
    }

    #[test]
    fn data_directory_must_be_a_directory() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("resources");
        let destination = temp.path().join("artifact-resources");
        std::fs::write(&source, "not a directory").unwrap();

        let error = reconcile_resource_mapping(temp.path(), "resources", &destination).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("executable package data directory must be a directory"),
            "{error:#}"
        );
        assert!(!destination.exists());
    }

    #[test]
    fn real_destination_directory_is_not_replaced() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("resources");
        let destination = temp.path().join("artifact-resources");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&destination).unwrap();
        std::fs::write(destination.join("keep.txt"), "keep").unwrap();

        let error = reconcile_resource_mapping(temp.path(), "resources", &destination).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("failed to map executable resources")
                || error.to_string().contains("destination already exists"),
            "{error:#}"
        );
        assert_eq!(
            std::fs::read_to_string(destination.join("keep.txt")).unwrap(),
            "keep"
        );
    }
}
