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

        let source = package.root_path.join(data_dir);
        let destination = artifact_parent.join(data_dir);
        validate_data_directory(&source)?;
        create_mapping_parent_directories(artifact_parent, data_dir)?;
        reconcile_resource_mapping_for_platform(&source, &destination)?;
    }
    Ok(())
}

fn create_mapping_parent_directories(artifact_parent: &Path, data_dir: &str) -> anyhow::Result<()> {
    let mut directory = artifact_parent.to_path_buf();
    let component_count = data_dir.split('/').count();
    for component in data_dir.split('/').take(component_count.saturating_sub(1)) {
        directory.push(component);
        match std::fs::symlink_metadata(&directory) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => anyhow::bail!(
                "cannot create executable data directory mapping: parent '{}' is not a real directory",
                directory.display()
            ),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&directory).with_context(|| {
                    format!(
                        "failed to create executable data directory mapping parent '{}'",
                        directory.display()
                    )
                })?;
            }
            Err(err) => {
                return Err(err).with_context(|| {
                    format!(
                        "failed to inspect executable data directory mapping parent '{}'",
                        directory.display()
                    )
                });
            }
        }
    }
    Ok(())
}

fn validate_data_directory(source: &Path) -> anyhow::Result<()> {
    match std::fs::metadata(source) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => anyhow::bail!(
            "executable package data directory must be a directory: '{}'",
            source.display()
        ),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!(
                "declared executable package data directory does not exist: '{}'",
                source.display()
            )
        }
        Err(err) => Err(err).with_context(|| {
            format!(
                "failed to inspect executable package data directory '{}'",
                source.display()
            )
        }),
    }
}

#[cfg(test)]
fn reconcile_resource_mapping(source: &Path, destination: &Path) -> anyhow::Result<()> {
    validate_data_directory(source)?;
    reconcile_resource_mapping_for_platform(source, destination)
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
            // for a junction whose target was removed. Deleting the reparse point
            // distinguishes that case from a Windows symbolic link without
            // following either one.
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

        reconcile_resource_mapping(&old_source, &destination).unwrap();
        reconcile_resource_mapping(&new_source, &destination).unwrap();

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

        reconcile_resource_mapping(&old_source, &destination).unwrap();
        std::fs::remove_dir(&old_source).unwrap();
        std::fs::create_dir(&new_source).unwrap();
        std::fs::write(new_source.join("current.txt"), "current").unwrap();

        reconcile_resource_mapping(&new_source, &destination).unwrap();

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

        let error = reconcile_resource_mapping(&source, &destination).unwrap_err();

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

        let error = reconcile_resource_mapping(&source, &destination).unwrap_err();

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
