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
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use indexmap::{set::MutableValues, IndexSet};
use log::{debug, trace};
use moonutil::{
    common::{
        DEP_PATH, DOT_MBT_DOT_MD, MOD_DIR, MOONCAKE_BIN, MOON_BIN_DIR, MOON_MOD_JSON,
        MOON_PKG_JSON, PKG_DIR,
    },
    compiler_flags::CC,
};
use petgraph::visit::DfsPostOrder;
use tracing::{instrument, warn, Level};

use crate::{
    build_plan::PrebuildInfo,
    cond_comp::{self, CompileCondition},
    model::{BuildPlanNode, BuildTarget, PackageId, TargetKind},
};

use super::{
    constructor::BuildPlanConstructor, BuildCStubsInfo, BuildPlanConstructError, BuildTargetInfo,
    LinkCoreInfo, MakeExecutableInfo,
};

impl<'a> BuildPlanConstructor<'a> {
    /// Add need to all prebuild scripts of the given package, and add edge to this node
    fn need_all_package_prebuild(&mut self, node: BuildPlanNode, pkg_id: PackageId) {
        let pkg = self.input.pkg_dirs.get_package(pkg_id);
        if let Some(prebuild) = &pkg.raw.pre_build {
            for i in 0..prebuild.len() {
                let prebuild_node = self.need_node(BuildPlanNode::RunPrebuild(pkg_id, i as u32));
                self.add_edge(node, prebuild_node);
            }
        }
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_check(
        &mut self,
        node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        // Check depends on `.mi` of all dependencies, which practically
        // means the Check of all dependencies.
        for dep in self
            .input
            .pkg_rel
            .dep_graph
            .neighbors_directed(target, petgraph::Direction::Outgoing)
        {
            let dep_node = self.need_node(BuildPlanNode::Check(dep));
            self.add_edge(node, dep_node);
        }

        self.need_all_package_prebuild(node, target.package);

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
            let dep_node = self.need_node(BuildPlanNode::BuildCore(dep));
            self.add_edge(node, dep_node);
        }

        // If the given target is a test, we will also need to generate the test driver.
        if target.kind.is_test() {
            let gen_test_info = BuildPlanNode::GenerateTestInfo(target);
            self.need_node(gen_test_info);
            self.add_edge(node, gen_test_info);
        }

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
            backend: self.build_env.target_backend,
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
            pb.iter().flat_map(|gen| {
                gen.output
                    .iter()
                    .filter(|x| x.ends_with(".mbt") || x.ends_with(DOT_MBT_DOT_MD))
                    .map(|x| Cow::Owned(pkg.root_path.join(x)))
            })
        });

        // Filter source files
        let source_files = cond_comp::filter_files(
            &pkg.raw,
            source_iter.chain(prebuild_output_iter),
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

        BuildTargetInfo {
            regular_files: regular_files.into_iter().collect(),
            whitebox_files: whitebox_files.into_iter().collect(),
            doctest_files: doctest_files.into_iter().collect(),
            warn_list,
            alert_list,
            specified_no_mi,
            patch_file,
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
    pub(super) fn build_link_c_stubs(
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

        // TODO: variable replacement
        // FIXME: This is getting long. fix spaghetti.
        let native_config = pkg.raw.link.as_ref().and_then(|x| x.native.as_ref());

        let stub_cc = native_config
            .and_then(|x| x.cc.as_ref())
            .map(|cc| CC::try_from_path(cc))
            .transpose()
            .map_err(|e| BuildPlanConstructError::FailedToSetStubCC(e, pkg.fqn.clone().into()))?;

        let cc_flags = native_config
            .and_then(|x| x.cc_flags.as_ref())
            .map(|x| {
                shlex::split(x).ok_or_else(|| {
                    BuildPlanConstructError::MalformedStubCCFlags(pkg.fqn.clone().into())
                })
            })
            .transpose()?
            .unwrap_or_default();

        let link_flags = native_config
            .and_then(|x| x.stub_cc_link_flags.as_ref())
            .map(|x| {
                shlex::split(x).ok_or_else(|| {
                    BuildPlanConstructError::MalformedStubCCLinkFlags(pkg.fqn.clone().into())
                })
            })
            .transpose()?
            .unwrap_or_default();

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
                TODO: virtual packages are not yet implemented here.
        */

        debug!("Building MakeExecutable for target: {:?}", target);
        debug!("Performing DFS post-order traversal to collect dependencies");
        // This DFS is shared by both LinkCore and MakeExecutable actions.
        let mut dfs = DfsPostOrder::new(&self.input.pkg_rel.dep_graph, target);
        // This is the link core sources
        let mut link_core_deps = IndexSet::new();
        // This is the C stub sources
        let mut c_stub_deps = Vec::new();
        // DFS itself
        while let Some(next) = dfs.next(&self.input.pkg_rel.dep_graph) {
            if next.kind == TargetKind::WhiteboxTest {
                // Replace whitebox tests, if any
                let source_target = next.package.build_target(TargetKind::Source);
                if let Some(source_idx) = link_core_deps.get_index_of(&source_target) {
                    let source_mut = link_core_deps
                        .get_index_mut2(source_idx)
                        .expect("Source index is valid");
                    *source_mut = next;
                    continue;
                } else {
                    // No source target found, resort to regular path
                }
            }

            // Regular package
            link_core_deps.insert(next);
            // If there's any C stubs, add it (native only)
            let pkg = self.input.pkg_dirs.get_package(next.package);
            trace!("DFS post iterated: {}", pkg.fqn);
            if self.build_env.target_backend.is_native() && !pkg.c_stub_files.is_empty() {
                c_stub_deps.push(next);
            }
        }

        let link_core_node = self.need_node(BuildPlanNode::LinkCore(target));

        // Add edges to all dependencies
        // Note that we have already replaced unnecessary dependencies
        for target in &link_core_deps {
            let dep_node = BuildPlanNode::BuildCore(*target);
            self.need_node(dep_node);
            self.add_edge(link_core_node, dep_node);
        }

        let targets = link_core_deps.into_iter().collect::<Vec<_>>();
        let link_core_info = LinkCoreInfo {
            linked_order: targets.clone(),
            // std: self.build_env.std, // TODO: move to per-package
        };
        self.res.link_core_info.insert(target, link_core_info);

        self.resolved_node(link_core_node);

        // Add edge from make exec to link core
        self.add_edge(make_exec_node, link_core_node);

        // Add dependencies of make exec
        for target in &c_stub_deps {
            let dep_node = self.need_node(BuildPlanNode::ArchiveCStubs(target.package));
            self.add_edge(make_exec_node, dep_node);
        }
        let c_stub_deps = c_stub_deps.into_iter().collect::<Vec<_>>();

        // Fill auxiliary flags for CC flags
        // TODO: variable replacement in flags
        // FIXME: This is getting long. fix spaghetti.
        let pkg = self.input.pkg_dirs.get_package(target.package);
        let native_config = pkg.raw.link.as_ref().and_then(|x| x.native.as_ref());
        let cc = native_config
            .and_then(|x| x.cc.as_ref())
            .map(|x| CC::try_from_path(x))
            .transpose()
            .map_err(|e| BuildPlanConstructError::FailedToSetCC(e, pkg.fqn.clone().into()))?;
        let c_flags = native_config
            .and_then(|x| x.cc_flags.as_ref())
            .map(|x| {
                shlex::split(x).ok_or_else(|| {
                    BuildPlanConstructError::MalformedCCFlags(pkg.fqn.clone().into())
                })
            })
            .transpose()?
            .unwrap_or_default();
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

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_bundle(
        &mut self,
        _node: BuildPlanNode,
        module_id: moonutil::mooncakes::ModuleId,
    ) -> Result<(), BuildPlanConstructError> {
        // Bundling a module gathers the build result of all its non-virtual packages
        for &pkg_id in self
            .input
            .pkg_dirs
            .packages_for_module(module_id)
            .expect("Module should exist")
            .values()
        {
            let pkg = self.input.pkg_dirs.get_package(pkg_id);
            if pkg.raw.virtual_pkg.is_some() {
                continue;
            }

            let build_node = BuildPlanNode::BuildCore(BuildTarget {
                package: pkg_id,
                kind: TargetKind::Source,
            });
            self.need_node(build_node);
            self.add_edge(_node, build_node);
        }

        Ok(())
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
        let check_node = self.need_node(BuildPlanNode::Check(target));
        self.add_edge(_node, check_node);
        self.resolved_node(_node);
        Ok(())
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_parse_mbti(
        &mut self,
        node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        let _ = (node, target);
        todo!("Build graph construction for ParseMbti nodes is not implemented");
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn build_build_docs(
        &mut self,
        _node: BuildPlanNode,
    ) -> Result<(), BuildPlanConstructError> {
        // For now, `moondoc` depends on *every check*, as specified in its
        // packages.json input. I guess bad things might happen if you don't?
        for (pkg_id, _) in self.input.pkg_dirs.all_packages() {
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
            if filename == MOON_MOD_JSON || filename == MOON_PKG_JSON {
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
/// This should mostly match the legacy code in behavior, sans the strange
/// `.ps1` replacement when encountering code.
fn handle_build_command_new(
    command: &str,
    mod_source: &Path,
    pkg_source: &Path,
    input_files: &[PathBuf],
    output_files: &[PathBuf],
) -> String {
    use std::fmt::Write;

    let mut reconstructed = String::new();

    let command = if let Some(command) = command.strip_prefix(":embed ") {
        reconstructed.push_str("moon tool embed ");
        command
    } else {
        command
    };

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
            // DUDE, WHAT THE FUCK IS THIS?!
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

    reconstructed
}
