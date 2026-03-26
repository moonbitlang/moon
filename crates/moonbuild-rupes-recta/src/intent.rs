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

//! User intent, a small shim to reduce friction between user commands and
//! build plan nodes.
//!
//! A user intent may map to 0 or more build plan nodes, based on the actual
//! package definition and status. This is for simplifying the CLI command
//! node generation logic.

use moonutil::{common::TargetBackend, mooncakes::ModuleId};
use tracing::warn;

use crate::{
    build_plan::InputDirective,
    cond_comp::get_file_target_backend,
    discover::DiscoveredPackage,
    model::{BuildPlanNode, BuildTarget, PackageId, TargetKind},
    resolve::ResolveOutput,
};

/// A concise set of user actions that expand into concrete BuildPlanNode groups.
#[derive(Clone, Copy, Debug)]
pub enum UserIntent {
    /// Build a package (produce either .core or an executable).
    Build(PackageId),
    /// Run a package (executable of Source target). Does not actually execute the output.
    Run(PackageId),
    /// Check a package (source/whitebox/blackbox).
    Check(PackageId),
    /// Prove a package (source only).
    Prove(PackageId),
    /// Test a package (emit test driver and build for all test targets).
    Test(PackageId),
    /// Bench a package (same node set shape as Test; runtime behavior differs elsewhere).
    Bench(PackageId),
    /// Bundle all non-virtual packages in a module.
    Bundle(ModuleId),
    /// Build docs for a single module.
    Doc(ModuleId),
    /// Generate .mbti for a package (non-virtual only).
    Info(PackageId),
}

impl UserIntent {
    /// Append the BuildPlanNode(s) represented by this intent to `out`.
    ///
    /// This does not deduplicate; callers can handle that if necessary.
    pub fn append_nodes(
        self,
        resolved: &ResolveOutput,
        out: &mut Vec<BuildPlanNode>,
        directive: &InputDirective,
        target_backend: TargetBackend,
    ) {
        match self {
            UserIntent::Build(pkg) => {
                let pkg_info = resolved.pkg_dirs.get_package(pkg);
                if !pkg_info.has_implementation() {
                    // Pure virtual package: compile its interface instead of building code
                    out.push(BuildPlanNode::BuildVirtual(pkg));
                } else {
                    let t = pkg.build_target(TargetKind::Source);
                    if is_linkable(pkg_info) {
                        out.push(BuildPlanNode::make_executable(t));
                    } else {
                        out.push(BuildPlanNode::build_core(t));
                    }
                }
            }
            UserIntent::Run(pkg) => {
                let pkg_info = resolved.pkg_dirs.get_package(pkg);
                if !pkg_info.has_implementation() {
                    // Pure virtual package: we can't do anything
                } else {
                    let t = pkg.build_target(TargetKind::Source);
                    out.push(BuildPlanNode::make_executable(t));
                }
            }
            UserIntent::Check(pkg) => {
                let pkg_info = resolved.pkg_dirs.get_package(pkg);
                if pkg_info.has_implementation() {
                    let source_target = pkg.build_target(TargetKind::Source);
                    // Backend support is target-specific: test-only imports can
                    // make whitebox/blackbox unrealizable even when the source
                    // target is still valid for the selected backend.
                    let source_supports_backend =
                        target_realizes_backend(resolved, source_target, target_backend);

                    // - Always check Source.
                    // - If this package is not a virtual implementation, we can
                    //   check tests (virtual impls cannot be tested).
                    // - When checking tests, always check blackbox tests, and
                    //   only check whitebox if it has related files.
                    out.push(BuildPlanNode::check(source_target));
                    if !pkg_info.is_virtual_impl()
                        && resolved.local_modules().contains(&pkg_info.module)
                    {
                        // If the package is in a local module, we check its
                        // blackbox/whitebox tests otherwise we skip checking
                        // its blackbox/whitebox tests

                        if has_whitebox_decl(resolved, pkg, directive) {
                            let whitebox_target = pkg.build_target(TargetKind::WhiteboxTest);
                            if !should_skip_test_target(
                                resolved,
                                source_supports_backend,
                                whitebox_target,
                                target_backend,
                            ) {
                                out.push(BuildPlanNode::check(whitebox_target));
                            }
                        }
                        let blackbox_target = pkg.build_target(TargetKind::BlackboxTest);
                        if !should_skip_test_target(
                            resolved,
                            source_supports_backend,
                            blackbox_target,
                            target_backend,
                        ) {
                            out.push(BuildPlanNode::check(blackbox_target));
                        }
                    }
                } else {
                    // Pure virtual package: compile its interface
                    out.push(BuildPlanNode::BuildVirtual(pkg));
                }
            }
            UserIntent::Prove(pkg) => {
                out.push(BuildPlanNode::prove(pkg.build_target(TargetKind::Source)));
            }
            UserIntent::Test(pkg) | UserIntent::Bench(pkg) => {
                let pkg_info = resolved.pkg_dirs.get_package(pkg);
                if !pkg_info.has_implementation() {
                    // Pure virtual package: we can't do anything
                } else if pkg_info.is_virtual_impl() {
                    // Virtual package implementation cannot be tested directly
                } else {
                    // `moon test` should still run realizable targets of the
                    // package even if test-only imports make some test targets
                    // backend-incompatible.
                    let source_supports_backend = target_realizes_backend(
                        resolved,
                        pkg.build_target(TargetKind::Source),
                        target_backend,
                    );

                    // Emit paired nodes per test target; skip Whitebox if no *_wbtest.mbt declared.
                    for &k in TargetKind::all_tests() {
                        if k == TargetKind::WhiteboxTest
                            && !has_whitebox_decl(resolved, pkg, directive)
                        {
                            continue;
                        }
                        let t = pkg.build_target(k);
                        if matches!(k, TargetKind::WhiteboxTest | TargetKind::BlackboxTest)
                            && should_skip_test_target(
                                resolved,
                                source_supports_backend,
                                t,
                                target_backend,
                            )
                        {
                            continue;
                        }
                        out.push(BuildPlanNode::make_executable(t));
                        out.push(BuildPlanNode::generate_test_info(t));
                    }
                }
            }
            UserIntent::Bundle(m) => {
                out.push(BuildPlanNode::Bundle(m));
            }
            UserIntent::Doc(module_id) => {
                out.push(BuildPlanNode::BuildDocs(module_id));
            }
            UserIntent::Info(pkg) => {
                let pkg_info = resolved.pkg_dirs.get_package(pkg);
                if !(pkg_info.is_virtual_impl() || pkg_info.is_virtual()) {
                    out.push(BuildPlanNode::GenerateMbti(
                        pkg.build_target(TargetKind::Source),
                    ));
                }
                // else: skip virtual packages to mirror `moon info` behavior
            }
        }
    }
}

#[inline]
fn is_linkable(pkg: &DiscoveredPackage) -> bool {
    pkg.raw.force_link || pkg.raw.link.is_some() || pkg.raw.is_main
}

/// Determine if any *_wbtest.mbt files are declared by the package.
fn has_whitebox_decl(
    resolved: &ResolveOutput,
    pkg_id: PackageId,
    directive: &InputDirective,
) -> bool {
    // If the user explicitly specified a patch file for whitebox tests, we consider
    // that as an indication that whitebox tests are desired.
    if let Some((target, _)) = &directive.specify_patch_file
        && target == &pkg_id.build_target(TargetKind::WhiteboxTest)
    {
        return true;
    }

    // Otherwise, check the source files for any whitebox test declarations.
    let pkg = resolved.pkg_dirs.get_package(pkg_id);
    pkg.source_files.iter().any(|p| {
        let file_stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let (_, with_target_stripped) = get_file_target_backend(file_stem);
        with_target_stripped.ends_with("_wbtest")
    })
}

fn should_skip_test_target(
    resolved: &ResolveOutput,
    source_supports_backend: bool,
    target: BuildTarget,
    target_backend: TargetBackend,
) -> bool {
    if !source_supports_backend || target_realizes_backend(resolved, target, target_backend) {
        return false;
    }

    warn_if_test_target_is_never_realizable(resolved, target);
    true
}

fn target_realizes_backend(
    resolved: &ResolveOutput,
    target: BuildTarget,
    target_backend: TargetBackend,
) -> bool {
    resolved
        .pkg_rel
        .realizable_supported_targets
        .get(&target)
        // Targets without edges are absent from the graph; in that case their
        // realizable support is just the package-level support.
        .unwrap_or(
            &resolved
                .pkg_dirs
                .get_package(target.package)
                .effective_supported_targets,
        )
        .contains(&target_backend)
}

fn warn_if_test_target_is_never_realizable(resolved: &ResolveOutput, target: BuildTarget) {
    let Some(realizable) = resolved.pkg_rel.realizable_supported_targets.get(&target) else {
        return;
    };
    if !realizable.is_empty() {
        return;
    }

    let pkg = resolved.pkg_dirs.get_package(target.package);
    let test_kind = match target.kind {
        TargetKind::WhiteboxTest => "whitebox",
        TargetKind::BlackboxTest => "blackbox",
        _ => return,
    };
    warn!(
        "Skipping {test_kind} tests for package `{}`: the test target is unrealizable on every backend because its dependency graph has no supported backend intersection",
        pkg.fqn
    );
}
