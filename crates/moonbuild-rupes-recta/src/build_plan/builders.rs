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

//! Individual build methods for different node types.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use indexmap::{IndexSet, set::MutableValues};
use moonutil::{
    common::{
        DEP_PATH, DOT_MBT_DOT_MD, MOD_DIR, MOON_BIN_DIR, MOON_MOD_JSON, MOONCAKE_BIN, PKG_DIR,
        is_moon_pkg,
    },
    compiler_flags::{CC, DETECTED_CC},
    mooncakes::ModuleId,
};
use regex::Regex;
use tracing::{Level, debug, instrument, trace, warn};

use crate::{
    build_plan::{BuildBundleInfo, FileDependencyKind, PrebuildInfo},
    cond_comp::{self, CompileCondition},
    discover::DiscoveredPackage,
    model::{BuildPlanNode, BuildTarget, PackageId, RunBackend, TargetKind},
};

use super::{
    BuildCStubsInfo, BuildPlanConstructError, BuildTargetInfo, LinkCoreInfo, MakeExecutableInfo,
    constructor::BuildPlanConstructor,
};

static BUILD_VAR_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$\{build\.([a-zA-Z0-9_]+)\}").expect("invalid build var regex"));

impl<'a> BuildPlanConstructor<'a> {
    fn module_prebuild_vars(&self, module: ModuleId) -> Option<&HashMap<String, String>> {
        self.prebuild_config
            .and_then(|cfg| cfg.module_outputs.get(&module))
            .map(|output| &output.vars)
    }

    fn replace_build_vars<'s>(
        &self,
        package: PackageId,
        module: ModuleId,
        value: &'s str,
    ) -> Cow<'s, str> {
        let Some(vars) = self.module_prebuild_vars(module) else {
            return Cow::Borrowed(value);
        };
        if vars.is_empty() {
            return Cow::Borrowed(value);
        }
        BUILD_VAR_REGEX.replace_all(value, |caps: &regex::Captures| {
            vars.get(caps.get(1).expect("build var regex has capture").as_str())
                .map(|s| s.as_str())
                .unwrap_or_else(|| {
                    let m_name = self.input.module_rel.mod_name_from_id(module);
                    let pkg_name = &self.input.pkg_dirs.get_package(package).fqn;
                    warn!(
                        "Build variable {} required in {} but not found in \
                        prebuild config output of module {}, \
                         replacing with empty string",
                        &caps[1], pkg_name, m_name
                    );

                    ""
                })
        })
    }

    /// Add need to all prebuild scripts of the given package, and add edge to this node
    ///
    /// According to the semantics, only local packages require prebuild scripts
    /// to be run. Remote packages should already have their prebuild outputs
    /// ready when they are fetched.
    fn need_all_package_prebuild(&mut self, node: BuildPlanNode, pkg_id: PackageId) {
        let pkg = self.input.pkg_dirs.get_package(pkg_id);
        if !self.input.local_modules().contains(&pkg.module) {
            return;
        }

        if let Some(prebuild) = &pkg.raw.pre_build {
            for i in 0..prebuild.len() {
                let prebuild_node = self.need_node(BuildPlanNode::RunPrebuild(pkg_id, i as u32));
                self.add_edge(node, prebuild_node);
            }
        }
        for i in 0..pkg.mbt_lex_files.len() {
            let prebuild_node = self.need_node(BuildPlanNode::RunMoonLexPrebuild(pkg_id, i as u32));
            self.add_edge(node, prebuild_node);
        }
        for i in 0..pkg.mbt_yacc_files.len() {
            let prebuild_node =
                self.need_node(BuildPlanNode::RunMoonYaccPrebuild(pkg_id, i as u32));
            self.add_edge(node, prebuild_node);
        }
    }

    /// Specify a need on the `.mi` of a dependency.
    ///
    /// This dynamically maps into either `Build`, `Check` or `BuildVirtual`
    /// nodes based on the property of the dependency package.
    fn need_mi_of_dep(&mut self, node: BuildPlanNode, dep: BuildTarget, check_only: bool) {
        // Skip stdlib packages when stdlib is injected, since we can use prebuilt .mi files.
        // When building the stdlib itself (build_env.std == false), treat stdlib packages
        // like normal packages and build their dependencies.
        if self.build_env.std && self.input.pkg_dirs.is_stdlib_package(dep.package) {
            return;
        }

        let pkg_info = self.input.pkg_dirs.get_package(dep.package);
        let dep_node = if pkg_info.is_virtual() {
            self.need_node(BuildPlanNode::BuildVirtual(dep.package))
        } else if check_only {
            self.need_node(BuildPlanNode::Check(dep))
        } else {
            self.need_node(BuildPlanNode::BuildCore(dep))
        };

        // Since this function is specifically for needing `.mi` files,
        // we can set this at the edge
        let edge_kind = if let BuildPlanNode::BuildCore(_) = dep_node {
            FileDependencyKind::BuildCore {
                mi: true,
                core: false,
            }
        } else {
            FileDependencyKind::AllFiles
        };
        self.add_edge_spec(node, dep_node, edge_kind);
    }

    fn need_virtual_if_necessary(
        &mut self,
        pkg: &DiscoveredPackage,
        node: BuildPlanNode,
        target: BuildTarget,
    ) {
        // If the given target is a virtual package with default implementation,
        // we need to build its interface first.
        if pkg.is_virtual() {
            let dep_node = self.need_node(BuildPlanNode::BuildVirtual(target.package));
            self.add_edge(node, dep_node);
        }

        // If the given target implements a virtual package, we need to build
        // the virtual package's interface first.
        if let Some(vpkg_id) = self.input.pkg_rel.virt_impl.get(target.package) {
            let dep_node = self.need_node(BuildPlanNode::BuildVirtual(*vpkg_id));
            self.add_edge(node, dep_node);
        }
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_check(
        &mut self,
        node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        let pkg = self.input.pkg_dirs.get_package(target.package);

        assert!(
            pkg.has_implementation(),
            "Checking a virtual package without implementation should use the \
            `BuildVirtual` action instead"
        );

        self.need_node(node);
        // Check depends on `.mi` of all dependencies, which practically
        // means the Check of all dependencies.
        for dep in self
            .input
            .pkg_rel
            .dep_graph
            .neighbors_directed(target, petgraph::Direction::Outgoing)
        {
            self.need_mi_of_dep(node, dep, true);
        }

        self.need_all_package_prebuild(node, target.package);

        self.need_virtual_if_necessary(pkg, node, target);
        self.populate_target_info(target);

        self.resolved_node(node);

        Ok(())
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_build(
        &mut self,
        node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        let pkg = self.input.pkg_dirs.get_package(target.package);

        assert!(
            pkg.has_implementation(),
            "Building a virtual package without implementation should use the \
            `BuildVirtual` action instead"
        );

        // Build depends on `.mi`` of all dependencies. Although Check can
        // also emit `.mi` files, since we're building, this action actually
        // means we need to build all dependencies.
        self.need_node(node);
        for dep in self
            .input
            .pkg_rel
            .dep_graph
            .neighbors_directed(target, petgraph::Direction::Outgoing)
        {
            self.need_mi_of_dep(node, dep, false);
        }

        // If the given target is a test, we will also need to generate the test driver.
        if target.kind.is_test() {
            let gen_test_info = BuildPlanNode::GenerateTestInfo(target);
            self.need_node(gen_test_info);
            self.add_edge_spec(
                node,
                gen_test_info,
                FileDependencyKind::GenerateTestInfo { meta: false },
            );
        }

        self.need_virtual_if_necessary(pkg, node, target);

        self.need_all_package_prebuild(node, target.package);

        self.populate_target_info(target);
        self.resolved_node(node);

        Ok(())
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_gen_test_info(
        &mut self,
        node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        self.need_node(node);

        self.populate_target_info(target);
        self.resolved_node(node);
        Ok(())
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn resolve_mbt_files_for_node(&self, target: BuildTarget) -> BuildTargetInfo {
        use crate::cond_comp::FileTestKind::*;
        use TargetKind::*;

        let pkg = self.input.pkg_dirs.get_package(target.package);
        let module = self.input.module_rel.module_info(pkg.module);

        // FIXME: Should we resolve test drivers' paths, or should we leave it
        // in the lowering phase? The path to the test driver depends on the
        // artifact layout, so we might not be able to do that here, unless we
        // add some kind of `SpecialFile::TestDriver` or something.
        let compile_condition = CompileCondition {
            optlevel: self.build_env.opt_level,
            test_kind: target.kind.into(),
            backend: self.build_env.target_backend.into(),
        };

        // Iterator of all existing source files in the package
        let source_iter = pkg.source_files.iter().map(|x| Cow::Borrowed(x.as_path()));

        // Iterator over all prebuild output files that are .mbt or .mbt.md
        // Or else they will not be picked up by the build system.
        //
        // This might emit duplicated files if the prebuilt file already exist
        // in the source directory. Should not affect the build system.
        // MAINTAINERS: These paths are relative.
        //
        // They need further normalizing to be the absolute paths required by
        // the build system. This is done below in the file inclusion process,
        // and assuming that all files are relative to package root path. if we
        // have to add more types of files in the future, especially if they are
        // *not* relative to package root, this should be changed to feed the
        // absolute path, or let the iteration item contain extra metadata about
        // what their roots are.
        let prebuild_output_iter = pkg.raw.pre_build.iter().flat_map(|pb| {
            pb.iter().flat_map(|r#gen| {
                r#gen
                    .output
                    .iter()
                    .filter(|x| x.ends_with(".mbt") || x.ends_with(DOT_MBT_DOT_MD))
                    .map(|x| Cow::Owned(pkg.root_path.join(x)))
            })
        });

        // MAINTAINERS: These predefined prebuild operations that have a fixed
        // meaning. If these are not needed in the future, they can be removed.
        let mbtlex_iter = pkg
            .mbt_lex_files
            .iter()
            .map(|x| Cow::Owned(x.with_extension("mbt")));
        let mbtyacc_iter = pkg
            .mbt_yacc_files
            .iter()
            .map(|x| Cow::Owned(x.with_extension("mbt")));

        // Filter source files
        let source_files = cond_comp::filter_files(
            &pkg.raw,
            source_iter
                .chain(prebuild_output_iter)
                .chain(mbtlex_iter)
                .chain(mbtyacc_iter),
            &compile_condition,
        );

        // Include files
        //
        // Source and prebuild might emit duplicated files if the prebuilt file
        // already exist in the source directory. We dedup it here.
        let mut regular_files = IndexSet::new();
        let mut whitebox_files = IndexSet::new();
        let mut doctest_files = IndexSet::new();
        let _filter_span = tracing::debug_span!("filtering_files").entered();
        for (file, file_kind) in source_files {
            match (target.kind, file_kind) {
                (Source | SubPackage | InlineTest, NoTest) => {
                    regular_files.insert(file.into_owned())
                }

                (WhiteboxTest, NoTest) => regular_files.insert(file.into_owned()),
                (WhiteboxTest, Whitebox) => whitebox_files.insert(file.into_owned()),

                (BlackboxTest, Blackbox) => regular_files.insert(file.into_owned()),
                (BlackboxTest, NoTest) => doctest_files.insert(file.into_owned()),

                _ => panic!(
                    "Unexpected file kind {:?} for target {:?} in package {}, \
                    this is a bug in the build system!",
                    file_kind, target, pkg.fqn
                ),
            };
        }
        if target.kind == BlackboxTest {
            // mbt.md files are also part of regular files
            for md_file in &pkg.mbt_md_files {
                regular_files.insert(md_file.clone());
            }
        }
        drop(_filter_span);

        // Sort the input, or the different order may cause n2 to view the input
        // file set as different than original.
        //
        // FIXME: we have already sorted them on discover, should we omit that?
        let _sort_span = tracing::debug_span!("sorting_files").entered();
        regular_files.sort();
        whitebox_files.sort();
        doctest_files.sort();
        drop(_sort_span);

        // Populate `alert_list` and `warn_list`
        // The list population is simply concatenating:
        //   module-level + package-level + commandline
        let warn_list = cat_opt(
            cat_opt(module.warn_list.clone(), pkg.raw.warn_list.as_deref()),
            self.build_env.warn_list.as_deref(),
        );
        let alert_list = cat_opt(
            cat_opt(module.alert_list.clone(), pkg.raw.alert_list.as_deref()),
            self.build_env.alert_list.as_deref(),
        );

        let specified_no_mi = self.input_directive.specify_no_mi_for == Some(target.package);
        let patch_file = self
            .input_directive
            .specify_patch_file
            .as_ref()
            .filter(|(specify_target, _)| specify_target == &target)
            .map(|(_, path)| path.clone());

        let mi_check_target = self.mi_check_target(target, pkg);

        BuildTargetInfo {
            regular_files: regular_files.into_iter().collect(),
            whitebox_files: whitebox_files.into_iter().collect(),
            doctest_files: doctest_files.into_iter().collect(),
            warn_list,
            alert_list,
            specified_no_mi,
            patch_file,
            check_mi_against: mi_check_target,
            value_tracing: self
                .input_directive
                .value_tracing
                .is_some_and(|pkg| pkg == target.package),
        }
    }

    /// Check if a given target needs to check `.mi` against another target.
    #[allow(clippy::manual_map)]
    fn mi_check_target(&self, target: BuildTarget, pkg: &DiscoveredPackage) -> Option<BuildTarget> {
        // Mi checks.
        // - A virtual package with a default implementation checks .mi with its
        //   own virtual interface declaration.
        // - A package implementing a virtual package checks .mi with the
        //   virtual package it implements.
        if target.kind == TargetKind::Source {
            if let Some(vpkg) = &pkg.raw.virtual_pkg {
                if vpkg.has_default {
                    Some(target.package.build_target(TargetKind::Source))
                } else {
                    unreachable!(
                        "A virtual package without default implementation should not have a build target info, thus should not reach here"
                    );
                }
            } else if let Some(implement) = self.input.pkg_rel.virt_impl.get(target.package) {
                Some(implement.build_target(TargetKind::Source))
            } else {
                None
            }
        } else {
            None
        }
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_build_c_stub(
        &mut self,
        node: BuildPlanNode,
        _target: PackageId,
        _index: u32,
    ) -> Result<(), BuildPlanConstructError> {
        // depends on nothing, but needs to be inserted into the list
        self.need_node(node);

        // We rely on the `link_c_stubs` action to resolve the C stub info
        // so this doesn't panic.
        self.resolved_node(node);
        Ok(())
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_archive_or_link_c_stubs(
        &mut self,
        node: BuildPlanNode,
        target: PackageId,
    ) -> Result<(), BuildPlanConstructError> {
        // Resolve the C stub files
        let pkg = self.input.pkg_dirs.get_package(target);
        for i in 0..pkg.c_stub_files.len() {
            let build_node = self.need_node(BuildPlanNode::BuildCStub(target, i as u32));
            self.add_edge(node, build_node);
        }

        // If we're tcc run, also depend on the runtime library
        if self.build_env.target_backend == RunBackend::NativeTccRun {
            let make_exec_node = self.need_node(BuildPlanNode::BuildRuntimeLib);
            self.add_edge(node, make_exec_node);
        }

        // Populate C stub info
        let native_config = pkg.raw.link.as_ref().and_then(|x| x.native.as_ref());

        let stub_cc = native_config
            .and_then(|native| native.stub_cc.as_ref())
            .map(|s| self.replace_build_vars(target, pkg.module, s))
            .map(|replaced| {
                CC::try_from_path(replaced.as_ref()).map_err(|e| {
                    BuildPlanConstructError::FailedToSetStubCC(e, pkg.fqn.clone().into())
                })
            })
            .transpose()?;

        let cc_flags = native_config
            .and_then(|native| native.stub_cc_flags.as_ref())
            .map(|s| self.replace_build_vars(target, pkg.module, s))
            .map(|replaced| {
                shlex::split(replaced.as_ref()).ok_or_else(|| {
                    BuildPlanConstructError::MalformedStubCCFlags(pkg.fqn.clone().into())
                })
            })
            .transpose()?
            .unwrap_or_default();

        let mut link_flags = native_config
            .and_then(|native| native.stub_cc_link_flags.as_ref())
            .map(|s| self.replace_build_vars(target, pkg.module, s))
            .map(|replaced| {
                shlex::split(replaced.as_ref()).ok_or_else(|| {
                    BuildPlanConstructError::MalformedStubCCLinkFlags(pkg.fqn.clone().into())
                })
            })
            .transpose()?
            .unwrap_or_default();

        self.propagate_link_config(stub_cc.as_ref(), std::iter::once(target), &mut link_flags);

        let c_info = BuildCStubsInfo {
            stub_cc,
            cc_flags,
            link_flags,
        };
        self.res.c_stubs_info.insert(target, c_info);
        self.resolved_node(node);

        Ok(())
    }

    /// Performs the construction of two actions in consecutive: Make Executable
    /// and Link Core.
    ///
    /// The two actions are always created together (Link Core is always a
    /// direct dependency of Make Executable, and there's no other actions that
    /// depends on Link Core), and both actions require traversing through the
    /// list of dependencies, so it's better to create both nodes at once,
    /// instead of in separate functions.
    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_make_exec_link_core(
        &mut self,
        make_exec_node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        /*
            Link-core requires traversing all output of the current package's
            all transitive dependencies, and emitting them in DFS post-order.

            There are a couple of replacements needed to be done when the
            traversal completes:
            - Whitebox tests need to replace the normal package in the
                dependency graph (at the same position as the normal package).
                This is technically a circular dependency but anyway :)
            - Virtual package overrides need to replace their overridden
                packages in the dependency graph. This is done by not adding
                virtual packages at all when collecting the targets.
        */

        debug!("Building MakeExecutable for target: {:?}", target);
        debug!("Performing DFS post-order traversal to collect dependencies");

        // ====== Link Core =====

        // This DFS is shared by both LinkCore and MakeExecutable actions.
        let (link_core_deps, c_stub_deps, abort_overridden) = self.dfs_link_core_sources(target)?;

        let link_core_node = self.need_node(BuildPlanNode::LinkCore(target));

        // Add edges to all dependencies
        // Note that we have already replaced unnecessary dependencies
        for target in &link_core_deps {
            let dep_node = BuildPlanNode::BuildCore(*target);
            self.need_node(dep_node);
            self.add_edge_spec(
                link_core_node,
                dep_node,
                FileDependencyKind::BuildCore {
                    mi: false,
                    core: true,
                },
            );
        }

        // Use DFS-built order directly (dependencies first, then dependents).
        let targets = link_core_deps.iter().copied().collect::<Vec<_>>();
        let link_core_info = LinkCoreInfo {
            linked_order: targets.clone(),
            abort_overridden,
            // std: self.build_env.std, // Can move std/nostd to per-package info
        };
        self.res.link_core_info.insert(target, link_core_info);

        self.resolved_node(link_core_node);

        // ===== Make Executable =====

        // Add edge from make exec to link core
        self.add_edge(make_exec_node, link_core_node);

        // Add dependencies of make exec
        for target in &c_stub_deps {
            let dep_node = self.need_node(BuildPlanNode::ArchiveOrLinkCStubs(*target));
            self.add_edge(make_exec_node, dep_node);
        }
        let c_stub_deps = c_stub_deps.into_iter().collect::<Vec<_>>();

        // Fill auxiliary flags for CC flags
        let pkg = self.input.pkg_dirs.get_package(target.package);
        let native_config = pkg.raw.link.as_ref().and_then(|x| x.native.as_ref());
        let cc = native_config
            .and_then(|native| native.cc.as_ref())
            .map(|s| self.replace_build_vars(target.package, pkg.module, s))
            .map(|replaced| {
                CC::try_from_path(replaced.as_ref())
                    .map_err(|e| BuildPlanConstructError::FailedToSetCC(e, pkg.fqn.clone().into()))
            })
            .transpose()?;
        let mut c_flags = native_config
            .and_then(|native| native.cc_flags.as_ref())
            .map(|s| self.replace_build_vars(target.package, pkg.module, s))
            .map(|replaced| {
                shlex::split(replaced.as_ref()).ok_or_else(|| {
                    BuildPlanConstructError::MalformedCCFlags(pkg.fqn.clone().into())
                })
            })
            .transpose()?
            .unwrap_or_default();

        // Also include native.cc_link_flags (linker args) in the final native link flags
        if let Some(mut link_flags) = native_config
            .and_then(|native| native.cc_link_flags.as_ref())
            .map(|s| self.replace_build_vars(target.package, pkg.module, s))
            .map(|replaced| {
                shlex::split(replaced.as_ref()).ok_or_else(|| {
                    BuildPlanConstructError::MalformedCCLinkFlags(pkg.fqn.clone().into())
                })
            })
            .transpose()?
        {
            c_flags.append(&mut link_flags);
        }

        let mut link_pkgs: Vec<PackageId> = targets.iter().map(|x| x.package).collect();
        if !link_pkgs.contains(&target.package) {
            link_pkgs.push(target.package);
        }
        self.propagate_link_config(cc.as_ref(), link_pkgs.into_iter(), &mut c_flags);

        let v = MakeExecutableInfo {
            link_c_stubs: c_stub_deps.clone(),
            cc,
            c_flags,
        };
        self.res.make_executable_info.insert(target, v);

        // Native backends also needs a runtime library
        if self.build_env.target_backend.is_native() {
            let rt_node = self.need_node(BuildPlanNode::BuildRuntimeLib);
            self.add_edge(make_exec_node, rt_node);
        }

        self.resolved_node(make_exec_node);

        Ok(())
    }

    fn dfs_link_core_sources(
        &mut self,
        target: BuildTarget,
    ) -> Result<(IndexSet<BuildTarget>, IndexSet<PackageId>, bool), BuildPlanConstructError> {
        // This DFS is shared by both LinkCore and MakeExecutable actions.
        let vp_info = self.input.pkg_rel.virtual_users.get(target.package);

        let abort = if self.build_env.std {
            self.input.pkg_dirs.abort_pkg()
        } else {
            None
        };
        let abort_override_pkg =
            abort.and_then(|abort| vp_info.and_then(|vu| vu.overrides.get(abort).copied()));

        // This is the link core sources
        let mut link_core_deps: IndexSet<BuildTarget> = IndexSet::new();
        // This is the C stub sources
        //
        // Since a package don't have separate C stub for different test targets,
        // we only need to record the package IDs here.
        //
        // Additionally, if we don't dedup it here, we will see C stub for the
        // package itself and its blackbox test target both being added, which
        // is redundant.
        let mut c_stub_deps: IndexSet<PackageId> = IndexSet::new();
        // Whether `moonbitlang/core/abort` is overridden
        let abort_overridden = abort_override_pkg.is_some();

        let graph = &self.input.pkg_rel.dep_graph;

        // Topo sort via DFS postorder
        let mut visited: HashSet<BuildTarget> = HashSet::new(); // pre-order visited
        let mut emitted: HashSet<BuildTarget> = HashSet::new(); // post-order emitted
        let mut stack: Vec<(BuildTarget, bool)> = Vec::new(); // bool = expanded marker

        // Seed with the root target
        stack.push((target, false));

        while let Some((curr, expanded)) = stack.pop() {
            if !expanded {
                // Pre-order processing
                // Check if already visited (before override resolution)
                if visited.contains(&curr) {
                    continue;
                }
                visited.insert(curr);

                // Resolve virtual overrides at pre-order for this node
                let mut node = curr;
                if let Some(vp_info) = vp_info
                    && let Some(&override_pkg) = vp_info.overrides.get(node.package)
                {
                    trace!(
                        from = ?node.package,
                        to = ?override_pkg,
                        "Overriding virtual package"
                    );
                    node = BuildTarget {
                        package: override_pkg,
                        kind: TargetKind::Source,
                    };
                }

                // Skip abort entirely
                if abort.is_some_and(|x| node.package == x) {
                    continue;
                }

                trace!(?node, "Pre-order: push marker, then schedule children");
                // Push post-order marker
                stack.push((node, true));

                // Gather dependencies (outgoing neighbors) and sort deterministically
                let mut deps: Vec<BuildTarget> = graph
                    .neighbors_directed(node, petgraph::Direction::Outgoing)
                    .filter(|dep| {
                        // Skip stdlib packages because they are always linked implicitly
                        // only when stdlib is injected. When building stdlib itself, keep them.
                        !self.build_env.std || !self.input.pkg_dirs.is_stdlib_package(dep.package)
                    })
                    .collect();

                if curr == target
                    && let Some(override_pkg) = abort_override_pkg
                {
                    let override_target = BuildTarget {
                        package: override_pkg,
                        kind: TargetKind::Source,
                    };
                    if !deps.contains(&override_target) {
                        deps.push(override_target);
                    }
                }

                deps.sort_by(|a, b| {
                    let pa = self.input.pkg_dirs.get_package(a.package);
                    let pb = self.input.pkg_dirs.get_package(b.package);
                    pa.fqn.cmp(&pb.fqn).then_with(|| a.kind.cmp(&b.kind))
                });

                // Push dependencies in reverse order so lexicographically smallest is processed first.
                // Skip already-visited nodes as an optimization (they would be filtered by the guard above anyway).
                for dep in deps.into_iter().rev() {
                    if !visited.contains(&dep) {
                        stack.push((dep, false));
                    }
                }
            } else {
                // Post-order: emit after all dependencies
                let cur = curr;

                // Whitebox replacement: if a whitebox test exists, replace the source entry in-place.
                if cur.kind == TargetKind::WhiteboxTest {
                    let source_target = cur.package.build_target(TargetKind::Source);
                    if let Some(source_idx) = link_core_deps.get_index_of(&source_target) {
                        let source_mut = link_core_deps
                            .get_index_mut2(source_idx)
                            .expect("Source index is valid");
                        *source_mut = cur;

                        // Record emitted and collect c-stub if necessary
                        emitted.insert(cur);
                        let pkg = self.input.pkg_dirs.get_package(cur.package);
                        if self.build_env.target_backend.is_native() && !pkg.c_stub_files.is_empty()
                        {
                            c_stub_deps.insert(cur.package);
                        }
                        continue;
                    }
                    // If source not found yet, fall through to regular insertion.
                }

                if emitted.contains(&cur) {
                    continue;
                }

                let pkg = self.input.pkg_dirs.get_package(cur.package);
                if !pkg.has_implementation() {
                    return Err(BuildPlanConstructError::NoImplementationForVirtualPackage {
                        package: self.input.pkg_dirs.fqn(target.package).clone().into(),
                        dep: self.input.pkg_dirs.fqn(cur.package).clone().into(),
                    });
                }

                link_core_deps.insert(cur);
                emitted.insert(cur);
                trace!(?cur, "Post-order: emitted");

                if self.build_env.target_backend.is_native() && !pkg.c_stub_files.is_empty() {
                    c_stub_deps.insert(cur.package);
                }
            }
        }

        Ok((link_core_deps, c_stub_deps, abort_overridden))
    }

    /// Propagate the link configuration of the packages in dependency to the output list
    fn propagate_link_config(
        &self,
        cc: Option<&CC>,
        pkgs: impl Iterator<Item = PackageId>,
        out: &mut Vec<String>,
    ) {
        let Some(prebuild) = self.prebuild_config else {
            return;
        };
        let is_msvc_like = cc.unwrap_or(&*DETECTED_CC).is_msvc();
        for pkg in pkgs {
            let Some(link_config) = prebuild.package_configs.get(&pkg) else {
                continue;
            };

            let link_flags = link_config
                .link_flags
                .as_ref()
                .and_then(|x| shlex::split(x));
            if let Some(link_flags) = link_flags {
                out.extend(link_flags);
            }

            for lib in &link_config.link_libs {
                if is_msvc_like {
                    out.push(format!("{lib}.lib"));
                } else {
                    out.push(format!("-l{lib}"));
                }
            }

            for path in &link_config.link_search_paths {
                if is_msvc_like {
                    out.push(format!("/LIBPATH:{path}"));
                } else {
                    out.push(format!("-L{path}"));
                }
            }
        }
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_bundle(
        &mut self,
        _node: BuildPlanNode,
        module_id: ModuleId,
    ) -> Result<(), BuildPlanConstructError> {
        // Bundling a module gathers the build result of all its non-virtual packages, in topo order
        let topo_sorted_pkgs = self.topo_sort_module_packages(module_id);
        let mut bundle_targets = Vec::new();
        for target in topo_sorted_pkgs.into_iter() {
            let pkg = self.input.pkg_dirs.get_package(target.package);
            if !pkg.has_implementation() {
                trace!(
                    ?module_id,
                    ?target,
                    "skipping bundle target without implementation"
                );
                continue;
            }

            let build_node = BuildPlanNode::BuildCore(target);
            trace!(?module_id, ?target, "enqueuing bundle dependency");
            self.need_node(build_node);
            self.add_edge_spec(
                _node,
                build_node,
                FileDependencyKind::BuildCore {
                    mi: false,
                    core: true,
                },
            );

            if pkg.is_virtual() {
                trace!(
                    ?module_id,
                    ?target,
                    "skipping including as build target for virtual package"
                );
                continue;
            }
            bundle_targets.push(target);
        }
        trace!(
            ?module_id,
            count = bundle_targets.len(),
            "recording bundle targets"
        );
        self.res
            .bundle_info
            .insert(module_id, BuildBundleInfo { bundle_targets });

        Ok(())
    }

    /// List all packages in the module in topological order.
    ///
    /// This is a DFS that limits its traversal to only packages within the module.
    fn topo_sort_module_packages(&self, module_id: ModuleId) -> Vec<BuildTarget> {
        let pkg_map = self
            .input
            .pkg_dirs
            .packages_for_module(module_id)
            .expect("Must exist");

        let cmp_by_fqn = |a: &PackageId, b: &PackageId| {
            let pkg_a = self.input.pkg_dirs.get_package(*a);
            let pkg_b = self.input.pkg_dirs.get_package(*b);
            pkg_a.fqn.cmp(&pkg_b.fqn)
        };

        // Seed the DFS with packages sorted by FQN to ensure deterministic traversal.
        let mut seeds: Vec<_> = pkg_map.values().copied().collect();
        seeds.sort_by(cmp_by_fqn);

        let graph = &self.input.pkg_rel.dep_graph;
        let mut ordered = Vec::new();
        let mut visited = HashSet::new();
        let mut stack = Vec::new();

        for pkg_id in seeds {
            let target = pkg_id.build_target(TargetKind::Source);
            if visited.contains(&target) {
                continue;
            }

            // Classic iterative DFS with an explicit stack so we control ordering precisely.
            stack.push((target, false));
            while let Some((node, expanded)) = stack.pop() {
                if expanded {
                    let pkg = self.input.pkg_dirs.get_package(node.package);
                    if pkg.module == module_id {
                        ordered.push(node);
                    }
                    continue;
                }

                if !visited.insert(node) {
                    continue;
                }

                stack.push((node, true));

                let mut deps: Vec<_> = graph
                    .neighbors_directed(node, petgraph::Direction::Outgoing)
                    .filter(|dep| dep.kind == TargetKind::Source)
                    .filter(|dep| {
                        let pkg = self.input.pkg_dirs.get_package(dep.package);
                        pkg.module == module_id
                    })
                    .collect();

                // Visit dependencies in sorted order, pushing reverse so the smallest comes off first.
                deps.sort_by(|a, b| cmp_by_fqn(&a.package, &b.package));

                for dep in deps.into_iter().rev() {
                    if !visited.contains(&dep) {
                        stack.push((dep, false));
                    }
                }
            }
        }

        ordered
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_runtime_lib(
        &mut self,
        _node: BuildPlanNode,
    ) -> Result<(), BuildPlanConstructError> {
        // Nothing specific to do here ;)
        self.resolved_node(_node);
        Ok(())
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_generate_mbti(
        &mut self,
        _node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        // Generate mbti relies on the `.mi` files spitted out by `moonc`, which
        // usually means `moonc check` instead of `moonc build`.
        self.need_mi_of_dep(_node, target, true);
        self.resolved_node(_node);
        Ok(())
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_parse_mbti(
        &mut self,
        node: BuildPlanNode,
        target: PackageId,
    ) -> Result<(), BuildPlanConstructError> {
        // Parse MBTI depends on the .mi of its dependencies
        let pkg = self.input.pkg_dirs.get_package(target);

        assert!(
            pkg.is_virtual(),
            "Only virtual packages can have their .mi parsed from .mbti files"
        );

        for dep in self.input.pkg_rel.dep_graph.neighbors_directed(
            target.build_target(TargetKind::Source),
            petgraph::Direction::Outgoing,
        ) {
            // Note: This depends on the `Check` node, which will be coalesced
            // to `Build` later if necessary.
            self.need_mi_of_dep(node, dep, true);
        }

        self.resolved_node(node);

        Ok(())
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_build_docs(
        &mut self,
        _node: BuildPlanNode,
    ) -> Result<(), BuildPlanConstructError> {
        // For now, `moondoc` depends on *every check*, as specified in its
        // packages.json input. I guess bad things might happen if you don't?
        for (pkg_id, _) in self.input.pkg_dirs.all_packages(true) {
            let check_node = self.need_node(BuildPlanNode::Check(
                pkg_id.build_target(TargetKind::Source),
            ));
            self.add_edge(_node, check_node);
        }
        Ok(())
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_run_prebuild(
        &mut self,
        node: BuildPlanNode,
        _package: PackageId,
        _index: u32,
    ) -> Result<(), BuildPlanConstructError> {
        // Theoretically there might be file-level dependencies between prebuild
        // commands, but we don't track it here, since it **will** be handled by
        // n2 which tracks file-level dependencies anyway.
        //
        // In this lowering process, we only handle the transformation of the
        // commands and files.
        //
        // For details, also see `/docs/dev/reference/prebuild.md`

        self.need_node(node);
        self.populate_prebuild(_package, _index);
        self.resolved_node(node);

        Ok(())
    }

    pub fn populate_prebuild(&mut self, package: PackageId, index: u32) {
        if self
            .res
            .prebuild_info
            .get(&package)
            .and_then(|v| v.get(index as usize).and_then(|x| x.as_ref()))
            .is_some()
        {
            // Already populated
            return;
        }

        let pkg = self.input.pkg_dirs.get_package(package);
        let module = &self.input.module_dirs[pkg.module];
        let prebuild_cmd =
            &pkg.raw.pre_build.as_ref().expect("Prebuild must exist")[index as usize];

        // Warn about suspicious outputs
        for output in prebuild_cmd.output.iter() {
            let output: &Path = output.as_ref();
            let Some(filename) = output.file_name().and_then(OsStr::to_str) else {
                continue;
            };

            // If the output is a moonbit source and it does not live in the current dir
            if (filename.ends_with(".mbt") || filename.ends_with(".mbt.md"))
                && output.parent() != Some("".as_ref())
            {
                warn!(
                    "Prebuild output '{}' is not in the package directory of package {}. \
                    Such behavior is not supported. \
                    The build system will not add it to the list of MoonBit files to compile. \
                    If you really intend to generate files for another package, \
                    please move the prebuild command to that package instead.",
                    output.display(),
                    pkg.fqn
                );
            }
            // If the file looks like a package manifest
            if filename == MOON_MOD_JSON || is_moon_pkg(filename) {
                warn!(
                    "Prebuild output '{}' of package {} looks like a package manifest file. \
                    Overwriting package manifests is not supported and may lead to unexpected behavior.",
                    output.display(),
                    pkg.fqn
                );
            }
        }

        // Normalize input and output paths. This is the relatively easy part.
        // FIXME: these paths are used again when determining input files in
        // `Self::populate_target_info`, should we cache them somewhere?
        let input_paths = prebuild_cmd
            .input
            .iter()
            .map(|x| pkg.root_path.join(x))
            .collect::<Vec<_>>();
        let output_paths = prebuild_cmd
            .output
            .iter()
            .map(|x| pkg.root_path.join(x))
            .collect::<Vec<_>>();

        // Handle command expansion and tokenization
        let command = handle_build_command_new(
            &prebuild_cmd.command,
            module,
            &pkg.root_path,
            &input_paths,
            &output_paths,
        );

        let info = PrebuildInfo {
            resolved_inputs: input_paths,
            resolved_outputs: output_paths,
            command,
        };

        let v = self.res.prebuild_info.entry(package).or_default();
        // Extend the vector if necessary
        while v.len() <= index as usize {
            v.push(None);
        }
        v[index as usize] = Some(info);
    }

    pub(super) fn build_lex_prebuild(
        &mut self,
        node: BuildPlanNode,
    ) -> Result<(), BuildPlanConstructError> {
        // Not much things to do here, moonlex is straightforward
        // the hassle is telling the build system we have the files to compile
        self.need_node(node);
        self.resolved_node(node);
        Ok(())
    }

    pub(super) fn build_yacc_prebuild(
        &mut self,
        node: BuildPlanNode,
    ) -> Result<(), BuildPlanConstructError> {
        // similar to lex prebuild
        self.need_node(node);
        self.resolved_node(node);
        Ok(())
    }
}

/// Concatenate two optional strings
fn cat_opt(x: Option<String>, y: Option<&str>) -> Option<String> {
    match (x, y) {
        (Some(mut a), Some(b)) => {
            a.push_str(b);
            Some(a)
        }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b.to_string()),
        (None, None) => None,
    }
}

static PREBUILD_AUTOMATA: LazyLock<aho_corasick::AhoCorasick> = LazyLock::new(|| {
    aho_corasick::AhoCorasickBuilder::new()
        .build([MOONCAKE_BIN, MOD_DIR, PKG_DIR, "$input", "$output"])
        .expect("Failed to build automata")
});

/// Handle the prebuild command replacement, outputs a single string that should
/// be `sh -c`'ed.
///
/// # Note about binary dependency artifacts
///
/// Currently, `moon` does not have direct support for referencing binary
/// dependency artifacts. Artifacts built by binary dependencies are placed in
/// either of these two locations:
///
/// - `<mod_source>/.mooncakes/__moonbin__/[bin-target-name]`, if the artifact
///   comes from a regular dependency from the official registry.
/// - At the root of the corresponding module's source directory, if the
///   artifact comes from a local dependency.
///
/// In prebuild commands, users can reference the former location using
/// `$mooncake_bin`. For the latter case, users need to manually specify the
/// relative path from the module source directory. For robustness, the relative
/// path needs to be resolved against the module source directory itself before
/// executing.
///
/// Windows puts another issue on top of this: binary dependencies are
/// Powershell scripts appended with `.ps1` extension. Therefore, we need to
/// resolve `argv[0]` and append a `.ps1` if such file exists.
fn handle_build_command_new(
    command: &str,
    mod_source: &Path,
    pkg_source: &Path,
    input_files: &[PathBuf],
    output_files: &[PathBuf],
) -> String {
    use std::fmt::Write;

    let mut reconstructed = String::new();

    let moon_bin_path = moonutil::BINARIES.moonbuild.to_string_lossy();

    let command = if let Some(command) = command.strip_prefix(":embed ") {
        reconstructed.push_str(&format!("{} tool embed ", moon_bin_path));
        command
    } else {
        command
    };

    // Perform replacements
    let mut last_end = 0usize;
    for magic in PREBUILD_AUTOMATA.find_iter(command) {
        // Commit previous segment
        if magic.start() > last_end {
            reconstructed.push_str(&command[last_end..magic.start()]);
        }

        // Insert replacement
        // See the IDs in CHECK_AUTOMATA
        match magic.pattern().as_usize() {
            // $mooncake_bin => <mod_source>/.mooncakes/__moonbin__
            0 => {
                let replacement = mod_source.join(DEP_PATH).join(MOON_BIN_DIR);
                write!(reconstructed, "{}", replacement.display()).expect("write can't fail");
            }
            // $mod_dir => <mod_source>
            1 => {
                write!(reconstructed, "{}", mod_source.display()).expect("write can't fail");
            }
            // $pkg_dir => <pkg_source>
            2 => {
                write!(reconstructed, "{}", pkg_source.display()).expect("write can't fail");
            }
            // $input => (existing)<input_1>, <input_2>, ...
            3 => {
                for (i, f) in input_files.iter().enumerate() {
                    if i != 0 {
                        write!(reconstructed, " ").expect("write can't fail");
                    }
                    write!(reconstructed, "{}", f.display()).expect("write can't fail");
                }
            }
            4 => {
                for (i, f) in output_files.iter().enumerate() {
                    if i != 0 {
                        write!(reconstructed, " ").expect("write can't fail");
                    }
                    write!(reconstructed, "{}", f.display()).expect("write can't fail");
                }
            }
            _ => unreachable!("Unexpected pattern id from CHECK_AUTOMATA"),
        }
        last_end = magic.end();
    }

    if last_end < command.len() {
        reconstructed.push_str(&command[last_end..]);
    }

    // Resolve argv[0]
    let argv0 = moonutil::shlex::get_argv0_native(&reconstructed);
    // Check if argv[0] looks like a relative path.
    let looks_like_path = argv0.contains(std::path::is_separator);
    let is_relative = looks_like_path && !Path::new(&argv0).is_absolute();
    // For relative paths, we need to resolve it against the package source
    // directory. Since we cannot easily splice the resolved path back into the
    // command string, we just prepend the source directory to the front of the
    // command.
    #[cfg(not(windows))]
    if is_relative {
        reconstructed = format!(
            "{}{}{}",
            mod_source.display(),
            std::path::MAIN_SEPARATOR,
            reconstructed
        );
    }
    // For windows, we also need to check if the resolved path with `.ps1` exists.
    #[cfg(windows)]
    if is_relative {
        let resolved_path_ps1 = dunce::canonicalize(mod_source.join(format!("{}.ps1", argv0)));
        if let Ok(new_argv0) = resolved_path_ps1
            && new_argv0.is_file()
        {
            use moonutil::shlex::split_argv0_windows;

            let (_argv0, rest) = split_argv0_windows(&reconstructed);
            // This is safe because '"' is not a valid path character on Windows,
            // and the original argv[0] must be a path-like string.
            reconstructed = format!("powershell \"{}\" {}", new_argv0.display(), rest);
        }
    }

    reconstructed
}
