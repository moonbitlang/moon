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

//! Actual implementation of `moon info` command.

use std::path::Path;

use anyhow::Context;
use colored::Colorize;
use indexmap::IndexMap;
use moonbuild::expect::write_diff;
use moonbuild_rupes_recta::{model::BuildPlanNode, pkg_name::PackageFQN};
use moonutil::common::{MBTI_GENERATED, TargetBackend};
use sha2::Digest;
use tracing::error;

use crate::rr_build::BuildMeta;

/// Promote the given build run's info results to their respective package directories.
pub fn promote_info_results(meta: &BuildMeta) {
    for (&node, artifact) in &meta.artifacts {
        let BuildPlanNode::GenerateMbti(target) = node else {
            continue;
        };
        assert!(
            artifact.artifacts.len() == 1,
            "mbti generation should only produce one artifact"
        );
        let mbti_path = artifact.artifacts.first().unwrap();

        let package_path = &meta
            .resolve_output
            .pkg_dirs
            .get_package(target.package)
            .root_path;
        let dest_path = package_path.join(MBTI_GENERATED);

        match std::fs::copy(mbti_path, &dest_path) {
            Ok(_) => {}
            Err(e) => {
                error!(
                    "Failed to copy generated mbti file from {} to {}: {}",
                    mbti_path.display(),
                    dest_path.display(),
                    e
                );
            }
        }
    }
}

struct PackageOutputGroup<'a> {
    pkg_name: &'a PackageFQN,
    backend_files: IndexMap<TargetBackend, &'a Path>,
}

impl<'a> PackageOutputGroup<'a> {
    fn new(pkg_name: &'a PackageFQN) -> Self {
        Self {
            pkg_name,
            backend_files: IndexMap::new(),
        }
    }

    fn insert(&mut self, backend: TargetBackend, path: &'a Path) {
        self.backend_files.insert(backend, path);
    }
}

/// Compare the outputs of different targets for consistency. `canonical` is the
/// target backend to use as the reference.
///
/// Prints any differences found, and returns `true` if all outputs are identical.
pub fn compare_info_outputs<'a>(
    it: impl Iterator<Item = &'a (TargetBackend, BuildMeta)>,
    canonical: TargetBackend,
) -> anyhow::Result<bool> {
    // First, transpose the data structure to group by package
    let mut transposed = IndexMap::<_, PackageOutputGroup>::new();
    for (backend, meta) in it {
        for (&node, artifact) in &meta.artifacts {
            let BuildPlanNode::GenerateMbti(target) = node else {
                continue;
            };
            assert!(
                artifact.artifacts.len() == 1,
                "mbti generation should only produce one artifact"
            );
            let mbti_path = artifact.artifacts.first().unwrap();
            transposed
                .entry(target.package)
                .or_insert_with(|| {
                    let fqn = &meta.resolve_output.pkg_dirs.get_package(target.package).fqn;
                    PackageOutputGroup::new(fqn)
                })
                .insert(*backend, mbti_path);
        }
    }

    // For each package, compare the outputs across different backends
    let mut identical = true;
    for (_package, group) in transposed {
        // Prefer the canonical backend as the reference. If not present, pick the first one.
        // If a package has 0 files, it should not reach here, so unwrap is safe.
        let reference_backend = if group.backend_files.contains_key(&canonical) {
            canonical
        } else {
            *group
                .backend_files
                .keys()
                .next()
                .expect("No backend files found")
        };
        identical &= compare_info_output_for_package(reference_backend, &group)?;
    }

    Ok(identical)
}

struct MbtiOutput<'a> {
    filename: &'a Path,
    content: String,
    hash: [u8; 32],
}

/// Compare the outputs of different targets for a single package.
///
/// Return `true` if all outputs are identical, `false` otherwise.
fn compare_info_output_for_package(
    canonical: TargetBackend,
    group: &PackageOutputGroup,
) -> anyhow::Result<bool> {
    // Read the inputs for each backend
    let mut backend_contents = IndexMap::new();
    for (backend, path) in &group.backend_files {
        let content = std::fs::read_to_string(path).with_context(|| {
            format!(
                "Failed to read mbti file for package {} at {}",
                group.pkg_name,
                path.display()
            )
        })?;

        // MAINTAINERS: This hash is purely used for convenience, since sha2
        // is used elsewhere in the codebase. You can replace it with any hash
        // function. This is for easy grouping of identical contents.
        let hash0 = sha2::Sha256::digest(&content);
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&hash0);

        backend_contents.insert(
            *backend,
            MbtiOutput {
                filename: path,
                content,
                hash,
            },
        );
    }

    // Group by hashes
    let mut hash_groups: IndexMap<[u8; 32], Vec<TargetBackend>> = IndexMap::new();
    for (backend, mbti_output) in &backend_contents {
        hash_groups
            .entry(mbti_output.hash)
            .or_default()
            .push(*backend);
    }
    let canonical_hash = &backend_contents
        .get(&canonical)
        .context("Canonical backend output not found")?
        .hash;

    if hash_groups.len() == 1 {
        // All outputs are identical
        return Ok(true);
    }

    // Now we can compare the groups
    println!(
        "#\n# Package {} has diverging interfaces across backends:",
        group.pkg_name
    );
    for (hash, backends) in &hash_groups {
        if hash == canonical_hash {
            continue;
        }

        let expected = &backend_contents
            .get(&canonical)
            .context("Canonical backend output not found")?;
        let actual = &backend_contents
            .get(&backends[0])
            .context("Backend output not found")?;

        println!("#\n# ---");
        println!(
            "{} {} {:?} {}",
            "---".bright_red(),
            group.pkg_name,
            canonical,
            format!("({})", expected.filename.display()).bright_black(),
        );
        for backend in backends {
            let actual = backend_contents
                .get(backend)
                .context("Backend output not found")?;
            println!(
                "{} {} {:?} {}",
                "+++".bright_green(),
                group.pkg_name,
                backend,
                format!("({})", actual.filename.display()).bright_black(),
            );
        }

        write_diff(&expected.content, &actual.content, 1, 2, std::io::stdout())?;
    }
    println!("# ------");

    Ok(false)
}
