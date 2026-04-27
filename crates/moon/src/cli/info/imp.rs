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

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use anyhow::Context;
use colored::Colorize;
use indexmap::IndexMap;
use moonbuild::expect::write_diff;
use moonbuild_rupes_recta::{
    ResolveOutput,
    model::{BuildPlanNode, PackageId},
    pkg_name::PackageFQN,
};
use moonutil::common::{MBTI_GENERATED, TargetBackend};
use sha2::Digest;
use tracing::error;

use crate::rr_build::BuildMeta;

pub(super) struct InfoOutputPlan {
    packages: IndexMap<PackageId, PackageOutputPlan>,
}

struct PackageOutputPlan {
    pkg_name: String,
    dest_path: PathBuf,
    canonical_backend: TargetBackend,
}

impl PackageOutputPlan {
    fn new_from_fqn(
        pkg_name: &PackageFQN,
        dest_path: PathBuf,
        canonical_backend: TargetBackend,
    ) -> Self {
        Self {
            pkg_name: pkg_name.to_string(),
            dest_path,
            canonical_backend,
        }
    }

    pub(super) fn canonical_backend(&self) -> TargetBackend {
        self.canonical_backend
    }
}

impl InfoOutputPlan {
    pub(super) fn canonical_backend_for(&self, package_id: &PackageId) -> Option<TargetBackend> {
        self.packages.get(package_id).map(|p| p.canonical_backend())
    }
}

pub(super) enum TargetKind {
    Canonical,
    Requested,
}

struct PackageOutputGroup<'a> {
    plan: &'a PackageOutputPlan,
    backend_files: IndexMap<TargetBackend, &'a Path>,
}

impl<'a> PackageOutputGroup<'a> {
    fn new(plan: &'a PackageOutputPlan) -> Self {
        Self {
            plan,
            backend_files: IndexMap::new(),
        }
    }

    fn insert(&mut self, backend: TargetBackend, path: &'a Path) {
        self.backend_files.insert(backend, path);
    }
}

pub(super) fn plan_info_outputs(
    resolve_output: &ResolveOutput,
    packages: impl IntoIterator<Item = PackageId>,
) -> InfoOutputPlan {
    let mut planned = IndexMap::new();

    for package_id in packages {
        planned.entry(package_id).or_insert_with(|| {
            let pkg = resolve_output.pkg_dirs.get_package(package_id);
            let module = resolve_output.module_rel.module_info(pkg.module);
            let canonical_backend = module
                .preferred_target
                .or(resolve_output.workspace_preferred_target)
                .unwrap_or_default();

            PackageOutputPlan::new_from_fqn(
                &pkg.fqn,
                pkg.root_path.join(MBTI_GENERATED),
                canonical_backend,
            )
        });
    }

    InfoOutputPlan { packages: planned }
}

/// Determine the canonical backend for writing `.mbti` files:
///
/// 1. Module's `preferred-backend` (if set in `moon.mod.json`)
/// 2. Workspace's `preferred-backend` (if set in `moon.work`)
/// 3. `wasm-gc` (default fallback)
///
/// Note: If a package's `supported-targets` does NOT include the canonical backend,
/// no `.mbti` file will be written. Users should set `preferred-backend` on the
/// module or workspace to match their supported targets.
impl InfoOutputPlan {
    pub(super) fn execution_targets(
        &self,
        requested_backends: &[TargetBackend],
    ) -> Vec<(TargetBackend, TargetKind)> {
        if self.packages.is_empty() {
            return Vec::new();
        }

        let canonical_backends: BTreeSet<_> = self
            .packages
            .values()
            .map(|plan| plan.canonical_backend)
            .collect();

        let mut targets: Vec<(TargetBackend, TargetKind)> = Vec::new();

        for &backend in requested_backends {
            targets.push((backend, TargetKind::Requested));
        }

        for backend in canonical_backends {
            if !requested_backends.contains(&backend) {
                targets.push((backend, TargetKind::Canonical));
            }
        }

        targets
    }
}

pub(super) fn promote_info_results<'a>(
    plan: &'a InfoOutputPlan,
    it: impl Iterator<Item = &'a (TargetBackend, BuildMeta)>,
) {
    for (_package, group) in collect_package_output_groups(plan, it) {
        let Some(source_path) = group
            .backend_files
            .get(&group.plan.canonical_backend)
            .copied()
        else {
            continue;
        };

        match std::fs::copy(source_path, &group.plan.dest_path) {
            Ok(_) => {}
            Err(e) => {
                error!(
                    "Failed to copy generated mbti file from {} to {}: {}",
                    source_path.display(),
                    group.plan.dest_path.display(),
                    e
                );
            }
        }
    }
}

pub(super) fn report_info_outputs<'a>(
    plan: &'a InfoOutputPlan,
    it: impl Iterator<Item = &'a (TargetBackend, BuildMeta)>,
    requested_backends: &[TargetBackend],
) -> anyhow::Result<()> {
    if requested_backends.is_empty() {
        return Ok(());
    }

    let requested_backends = requested_backends.iter().copied().collect::<BTreeSet<_>>();

    for (_package, group) in collect_package_output_groups(plan, it) {
        report_info_output_for_package(&requested_backends, &group)?;
    }

    Ok(())
}

fn collect_package_output_groups<'a>(
    plan: &'a InfoOutputPlan,
    it: impl Iterator<Item = &'a (TargetBackend, BuildMeta)>,
) -> IndexMap<PackageId, PackageOutputGroup<'a>> {
    let mut transposed = plan
        .packages
        .iter()
        .map(|(&package, output_plan)| (package, PackageOutputGroup::new(output_plan)))
        .collect::<IndexMap<_, _>>();

    for (backend, meta) in it {
        for (&node, artifact) in &meta.artifacts {
            let BuildPlanNode::GenerateMbti(target) = node else {
                continue;
            };
            let Some(group) = transposed.get_mut(&target.package) else {
                continue;
            };

            assert!(
                artifact.artifacts.len() == 1,
                "mbti generation should only produce one artifact"
            );
            let mbti_path = artifact.artifacts.first().unwrap();
            group.insert(*backend, mbti_path);
        }
    }

    transposed
}

type ContentHash = [u8; 32];

struct MbtiOutput<'a> {
    filename: &'a Path,
    content: String,
    hash: ContentHash,
}

fn read_backend_contents<'a>(
    group: &'a PackageOutputGroup<'a>,
) -> anyhow::Result<IndexMap<TargetBackend, MbtiOutput<'a>>> {
    let mut backend_contents = IndexMap::new();

    for (backend, path) in &group.backend_files {
        let content = std::fs::read_to_string(path).with_context(|| {
            format!(
                "Failed to read mbti file for package {} at {}",
                group.plan.pkg_name,
                path.display()
            )
        })?;

        let hash = content_hash(&content);

        backend_contents.insert(
            *backend,
            MbtiOutput {
                filename: path,
                content,
                hash,
            },
        );
    }

    Ok(backend_contents)
}

fn report_info_output_for_package(
    requested_backends: &BTreeSet<TargetBackend>,
    group: &PackageOutputGroup,
) -> anyhow::Result<()> {
    let backend_contents = read_backend_contents(group)?;
    let requested_outputs = requested_backends
        .iter()
        .copied()
        .filter(|backend| backend_contents.contains_key(backend))
        .collect::<Vec<_>>();

    if requested_outputs.is_empty() {
        return Ok(());
    }

    let canonical_backend = group.plan.canonical_backend;
    let canonical = backend_contents.get(&canonical_backend);
    let baseline_content = canonical
        .map(|output| output.content.as_str())
        .unwrap_or("");
    let baseline_hash = canonical
        .map(|output| output.hash)
        .unwrap_or_else(|| content_hash(""));
    let baseline_file_label = canonical
        .map(|output| format!("({})", output.filename.display()))
        .unwrap_or_else(|| "(no generated interface)".to_string());
    let hash_groups =
        group_requested_outputs_by_hash(&backend_contents, requested_outputs, canonical_backend)?;

    if hash_groups.keys().all(|hash| hash == &baseline_hash) {
        return Ok(());
    }

    if canonical.is_some() {
        println!(
            "#\n# Package {} has diverging interfaces across backends:",
            group.plan.pkg_name
        );
    } else {
        println!(
            "#\n# Package {} has requested interfaces different from canonical backend {:?}:",
            group.plan.pkg_name, canonical_backend
        );
    }

    for (hash, backends) in hash_groups {
        if hash == baseline_hash {
            continue;
        }

        let actual = backend_contents
            .get(&backends[0])
            .context("Backend output not found")?;

        println!("#\n# ---");
        println!(
            "{} {} {:?} {}",
            "---".bright_red(),
            group.plan.pkg_name,
            canonical_backend,
            baseline_file_label.bright_black(),
        );
        for backend in &backends {
            let actual = backend_contents
                .get(backend)
                .context("Backend output not found")?;
            println!(
                "{} {} {:?} {}",
                "+++".bright_green(),
                group.plan.pkg_name,
                backend,
                format!("({})", actual.filename.display()).bright_black(),
            );
        }

        write_diff(baseline_content, &actual.content, 1, 2, std::io::stdout())?;
    }

    println!("# ------");
    Ok(())
}

fn content_hash(content: &str) -> ContentHash {
    let hash0 = sha2::Sha256::digest(content);
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&hash0);
    hash
}

fn group_requested_outputs_by_hash(
    backend_contents: &IndexMap<TargetBackend, MbtiOutput<'_>>,
    requested_outputs: Vec<TargetBackend>,
    canonical_backend: TargetBackend,
) -> anyhow::Result<IndexMap<ContentHash, Vec<TargetBackend>>> {
    // Use a compact content hash as the grouping key so identical backend
    // interfaces share one diff in the report.
    let mut hash_groups: IndexMap<ContentHash, Vec<TargetBackend>> = IndexMap::new();

    for backend in requested_outputs {
        if backend == canonical_backend {
            continue;
        }

        let actual = backend_contents
            .get(&backend)
            .context("Requested backend output not found")?;
        hash_groups.entry(actual.hash).or_default().push(backend);
    }

    Ok(hash_groups)
}
