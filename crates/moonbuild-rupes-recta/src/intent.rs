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

//! User intent, a small shim from command semantics to requested artifacts.
//!
//! A user intent may map to zero or more logical results based on the resolved
//! package. Build planning, rather than the CLI adapter, selects the actions
//! that provide those results for the current backend.

use moonutil::{resolution::ModuleId, target::TargetBackend, user_log::UserLog};

use crate::{
    build_plan::{ArtifactKey, InputDirective},
    cond_comp::get_file_target_backend,
    discover::DiscoveredPackage,
    model::{BuildTarget, PackageId, TargetKind},
    resolve::ResolveOutput,
};

/// A concise set of user actions that expand into requested artifact groups.
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
    /// Bench a package (same artifact set as Test; runtime behavior differs elsewhere).
    Bench(PackageId),
    /// Bundle all non-virtual packages in a module.
    Bundle(ModuleId),
    /// Build docs for a single module.
    Doc(ModuleId),
    /// Generate .mbti for a package (non-virtual only).
    Info(PackageId),
}

impl UserIntent {
    /// Append the logical artifacts represented by this intent to `out`.
    ///
    /// This does not deduplicate; callers can handle that if necessary.
    pub fn append_artifacts(
        self,
        resolved: &ResolveOutput,
        out: &mut Vec<ArtifactKey>,
        user_log: &UserLog,
        directive: &InputDirective,
        target_backend: TargetBackend,
    ) {
        match self {
            UserIntent::Build(pkg) => {
                let pkg_info = resolved.pkg_dirs.get_package(pkg);
                if !pkg_info.has_implementation() {
                    // Pure virtual package: compile its interface instead of building code
                    out.push(ArtifactKey::VirtualContractMi { package: pkg });
                } else {
                    if is_linkable(pkg_info) {
                        out.push(ArtifactKey::Executable {
                            package: pkg,
                            target_kind: TargetKind::Source,
                        });
                    } else {
                        out.push(ArtifactKey::CoreIr {
                            package: pkg,
                            target_kind: TargetKind::Source,
                        });
                    }
                }
            }
            UserIntent::Run(pkg) => {
                let pkg_info = resolved.pkg_dirs.get_package(pkg);
                if !pkg_info.has_implementation() {
                    // Pure virtual package: we can't do anything
                } else {
                    out.push(ArtifactKey::Executable {
                        package: pkg,
                        target_kind: TargetKind::Source,
                    });
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
                    out.push(ArtifactKey::CheckMi {
                        package: pkg,
                        target_kind: TargetKind::Source,
                    });
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
                                user_log,
                            ) {
                                out.push(ArtifactKey::CheckMi {
                                    package: pkg,
                                    target_kind: TargetKind::WhiteboxTest,
                                });
                            }
                        }
                        let blackbox_target = pkg.build_target(TargetKind::BlackboxTest);
                        if !should_skip_test_target(
                            resolved,
                            source_supports_backend,
                            blackbox_target,
                            target_backend,
                            user_log,
                        ) {
                            out.push(ArtifactKey::CheckMi {
                                package: pkg,
                                target_kind: TargetKind::BlackboxTest,
                            });
                        }
                    }
                } else {
                    // Pure virtual package: compile its interface
                    out.push(ArtifactKey::VirtualContractMi { package: pkg });
                }
            }
            UserIntent::Prove(pkg) => {
                out.push(ArtifactKey::ProofWhyml {
                    package: pkg,
                    target_kind: TargetKind::Source,
                });
                out.push(ArtifactKey::ProofReport {
                    package: pkg,
                    target_kind: TargetKind::Source,
                });
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

                    // Request execution and test metadata per target; skip
                    // Whitebox if no *_wbtest.mbt is declared.
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
                                user_log,
                            )
                        {
                            continue;
                        }
                        out.push(ArtifactKey::Executable {
                            package: pkg,
                            target_kind: k,
                        });
                        out.push(ArtifactKey::GeneratedTestDriver {
                            package: pkg,
                            target_kind: k,
                        });
                        out.push(ArtifactKey::GeneratedTestMetadata {
                            package: pkg,
                            target_kind: k,
                        });
                    }
                    if target_backend == TargetBackend::Js {
                        out.push(ArtifactKey::NodeTestPackageConfig { package: pkg });
                    }
                }
            }
            UserIntent::Bundle(m) => {
                out.push(ArtifactKey::BundleResult { module: m });
            }
            UserIntent::Doc(module_id) => {
                out.push(ArtifactKey::DocsDir { module: module_id });
            }
            UserIntent::Info(pkg) => {
                let pkg_info = resolved.pkg_dirs.get_package(pkg);
                if !(pkg_info.is_virtual_impl() || pkg_info.is_virtual()) {
                    out.push(ArtifactKey::GeneratedMbti {
                        package: pkg,
                        target_kind: TargetKind::Source,
                    });
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
    user_log: &UserLog,
) -> bool {
    if !source_supports_backend || target_realizes_backend(resolved, target, target_backend) {
        return false;
    }

    warn_or_info_test_target_skip(resolved, target, target_backend, user_log);
    true
}

fn target_realizes_backend(
    resolved: &ResolveOutput,
    target: BuildTarget,
    target_backend: TargetBackend,
) -> bool {
    realizable_supported_backends(resolved, target).contains(&target_backend)
}

fn realizable_supported_backends(
    resolved: &ResolveOutput,
    target: BuildTarget,
) -> &indexmap::IndexSet<TargetBackend> {
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
}

fn warn_or_info_test_target_skip(
    resolved: &ResolveOutput,
    target: BuildTarget,
    target_backend: TargetBackend,
    user_log: &UserLog,
) {
    let realizable = realizable_supported_backends(resolved, target);

    let pkg = resolved.pkg_dirs.get_package(target.package);
    let test_kind = match target.kind {
        TargetKind::WhiteboxTest => "whitebox",
        TargetKind::BlackboxTest => "blackbox",
        _ => return,
    };

    if realizable.is_empty() {
        user_log.warn(format!(
            "Skipping {test_kind} tests for package `{}`: the test target is unrealizable on every backend because its dependency graph has no supported backend intersection",
            pkg.fqn
        ));
        return;
    }

    let mut supported_backends = realizable
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    supported_backends.sort();
    user_log.info(format!(
        "Skipping {test_kind} tests for package `{}` on backend `{}`: target is not realizable for this backend. Realizable backends: [{}]",
        pkg.fqn,
        target_backend,
        supported_backends.join(", ")
    ));
}
