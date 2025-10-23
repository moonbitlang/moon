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

use moonutil::mooncakes::ModuleId;

use crate::{
    discover::DiscoveredPackage,
    model::{BuildPlanNode, PackageId, TargetKind},
    resolve::ResolveOutput,
};

/// A concise set of user actions that expand into concrete BuildPlanNode groups.
///
/// Notes:
/// - Build emits either BuildCore(Source) or MakeExecutable(Source) depending on linkability.
/// - Check emits three Check nodes: Source, WhiteboxTest, BlackboxTest. If the package has no
///   implementation (pure virtual), it emits BuildVirtual instead.
/// - Test emits pairs per test target: MakeExecutable + GenerateTestInfo for
///   WhiteboxTest, BlackboxTest, InlineTest.
/// - Bundle emits a module-level Bundle node.
/// - Docs emits a single BuildDocs node.
/// - Info emits GenerateMbti(Source) for non-virtual packages.
/// - Run emits MakeExecutable(Source).
#[derive(Clone, Copy, Debug)]
pub enum UserIntent {
    /// Build a package (produce either .core or an executable).
    Build(PackageId),
    /// Run a package (executable of Source target).
    Run(PackageId),
    /// Check a package (source/whitebox/blackbox).
    Check(PackageId),
    /// Test a package (emit test driver and build for all test targets).
    Test(PackageId),
    /// Bench a package (same node set shape as Test; runtime behavior differs elsewhere).
    Bench(PackageId),
    /// Bundle all non-virtual packages in a module.
    Bundle(ModuleId),
    /// Build docs for the whole workspace universe.
    Docs,
    /// Generate .mbti for a package (non-virtual only).
    Info(PackageId),
}

impl UserIntent {
    /// Append the BuildPlanNode(s) represented by this intent to `out`.
    ///
    /// This does not deduplicate; callers can handle that if necessary.
    pub fn append_nodes(self, resolved: &ResolveOutput, out: &mut Vec<BuildPlanNode>) {
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
                    // - Always check Source.
                    // - If this package is not a virtual implementation, we can
                    //   check tests (virtual impls cannot be tested).
                    // - When checking tests, always check blackbox tests, and
                    //   only check whitebox if it has related files.
                    out.push(BuildPlanNode::check(pkg.build_target(TargetKind::Source)));
                    if !pkg_info.is_virtual_impl() {
                        if has_whitebox_decl(resolved, pkg) {
                            out.push(BuildPlanNode::check(
                                pkg.build_target(TargetKind::WhiteboxTest),
                            ));
                        }
                        out.push(BuildPlanNode::check(
                            pkg.build_target(TargetKind::BlackboxTest),
                        ));
                    }
                } else {
                    // Pure virtual package: compile its interface
                    out.push(BuildPlanNode::BuildVirtual(pkg));
                }
            }
            UserIntent::Test(pkg) | UserIntent::Bench(pkg) => {
                let pkg_info = resolved.pkg_dirs.get_package(pkg);
                if !pkg_info.has_implementation() {
                    // Pure virtual package: we can't do anything
                } else if pkg_info.is_virtual_impl() {
                    // Virtual package implementation cannot be tested directly
                } else {
                    // Emit paired nodes per test target; skip Whitebox if no *_wbtest.mbt declared.
                    for &k in TargetKind::all_tests() {
                        if k == TargetKind::WhiteboxTest && !has_whitebox_decl(resolved, pkg) {
                            continue;
                        }
                        let t = pkg.build_target(k);
                        out.push(BuildPlanNode::make_executable(t));
                        out.push(BuildPlanNode::generate_test_info(t));
                    }
                }
            }
            UserIntent::Bundle(m) => {
                out.push(BuildPlanNode::Bundle(m));
            }
            UserIntent::Docs => {
                out.push(BuildPlanNode::BuildDocs);
            }
            UserIntent::Info(pkg) => {
                let pkg_info = resolved.pkg_dirs.get_package(pkg);
                if !pkg_info.is_virtual() {
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
fn has_whitebox_decl(resolved: &ResolveOutput, pkg_id: PackageId) -> bool {
    let pkg = resolved.pkg_dirs.get_package(pkg_id);
    pkg.source_files.iter().any(|p| {
        let file_name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        file_name.ends_with("_wbtest.mbt")
    })
}
