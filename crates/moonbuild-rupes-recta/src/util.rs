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

//! A number of random utilities useful for debugging the project

use crate::discover::DiscoverResult;
use crate::pkg_solve::DepRelationship;
use crate::{
    build_plan::{BuildPlan, BuildPlanActionKey, PackagePrebuildKey, package_file_key},
    model::BuildPlanNode,
};
use moonutil::resolution::ResolvedEnv;
use petgraph::Direction;
use std::io::{self, Write};
use std::path::Path;

/// Strip trailing slashes from a path.
///
/// # Note
///
/// This is a **workaround** as there's no a stdlib function to do this
/// directly. It's abusing the behavior of `strip_prefix` which removes trailing
/// slash by forcing removal of an empty prefix (which does nothing to the path).
///
/// Related issues:
/// - `strip_prefix` behavior: https://github.com/rust-lang/rust/issues/148267
/// - Methods for normalizing paths: https://github.com/rust-lang/rust/issues/142503
pub(crate) fn strip_trailing_slash(path: &Path) -> &Path {
    path.strip_prefix("").unwrap_or(path)
}

/// Print a resolved environment as a DOT graph
pub fn print_resolved_env_dot(env: &ResolvedEnv, writer: &mut dyn Write) -> io::Result<()> {
    writeln!(writer, "digraph ResolvedEnv {{")?;
    writeln!(writer, "    rankdir=TB;")?;
    writeln!(writer, "    node [shape=box style=\"filled,rounded\"];")?;

    // Nodes: use ModuleId debug as ID and full source name as label
    for (id, src) in env.all_modules_and_id() {
        let node_id = format!("{:?}", id);
        let src_str = src.to_string();

        // Split long paths into multiple lines for better readability
        let label = if src_str.len() > 30 {
            // Split on common path separators and join with \n
            let parts: Vec<&str> = src_str.split('/').collect();
            if parts.len() > 1 {
                let mut lines = Vec::new();
                let mut current_line = String::new();
                for part in parts {
                    if current_line.len() + part.len() + 1 > 20 && !current_line.is_empty() {
                        lines.push(current_line);
                        current_line = part.to_string();
                    } else {
                        if !current_line.is_empty() {
                            current_line.push('/');
                        }
                        current_line.push_str(part);
                    }
                }
                if !current_line.is_empty() {
                    lines.push(current_line);
                }
                lines.join("\\n")
            } else {
                src_str.replace('/', "\\n")
            }
        } else {
            src_str
        };

        // Color-code based on module type
        let color = match src.source() {
            moonutil::resolution::ModuleSourceKind::Local(_) => "lightgreen",
            moonutil::resolution::ModuleSourceKind::Registry => "lightblue",
            moonutil::resolution::ModuleSourceKind::Git(_) => "lightyellow",
            moonutil::resolution::ModuleSourceKind::Stdlib(_) => "lightgray",
            moonutil::resolution::ModuleSourceKind::SingleFile(_) => "orange",
        };

        writeln!(
            writer,
            "    \"{}\" [label=\"{}\" fillcolor=\"{}\"];",
            node_id, label, color
        )?;
    }

    // Edges: dependencies with module IDs and dependency key labels
    for (from, _) in env.all_modules_and_id() {
        for (to, key) in env.deps_keyed(from) {
            let from_id = format!("{:?}", from);
            let to_id = format!("{:?}", to);
            writeln!(
                writer,
                "    \"{}\" -> \"{}\" [label=\"{}\"];",
                from_id, to_id, key
            )?;
        }
    }
    writeln!(writer, "}}")?;
    Ok(())
}

/// Print a dependency relationship of build targets as a DOT graph, resolving package IDs to full names
pub fn print_dep_relationship_dot(
    dep: &DepRelationship,
    packages: &DiscoverResult,
    writer: &mut dyn Write,
) -> io::Result<()> {
    writeln!(writer, "digraph DepRelationship {{")?;
    writeln!(writer, "    rankdir=TB;")?;
    writeln!(writer, "    node [shape=box style=\"filled,rounded\"];")?;

    // Nodes: use BuildTarget debug as ID, label with full package FQN and kind
    for target in dep.dep_graph.nodes() {
        let id = format!("{:?}@{:?}", target.package, target.kind);
        let fqn = packages.fqn(target.package);
        let fqn_str = fqn.to_string();

        // Split long package names into multiple lines for better readability
        let label = if fqn_str.len() > 25 {
            let parts: Vec<&str> = fqn_str.split('/').collect();
            if parts.len() > 1 {
                format!("{}\\n{:?}", parts.join("\\n"), target.kind)
            } else {
                format!("{}\\n{:?}", fqn_str, target.kind)
            }
        } else {
            format!("{}\\n{:?}", fqn_str, target.kind)
        };

        // Color-code based on target kind
        let color = match target.kind {
            crate::model::TargetKind::Source => "lightblue",
            crate::model::TargetKind::SubPackage => "lightgreen",
            crate::model::TargetKind::WhiteboxTest => "lightyellow",
            crate::model::TargetKind::BlackboxTest => "lightcoral",
            crate::model::TargetKind::InlineTest => "lightpink",
        };

        writeln!(
            writer,
            "    \"{}\" [label=\"{}\" fillcolor=\"{}\"];",
            id, label, color
        )?;
    }

    // Edges: use node IDs with alias label
    for from in dep.dep_graph.nodes() {
        for to in dep.dep_graph.neighbors_directed(from, Direction::Outgoing) {
            if let Some(edge) = dep.dep_graph.edge_weight(from, to) {
                let id_from = format!("{:?}@{:?}", from.package, from.kind);
                let id_to = format!("{:?}@{:?}", to.package, to.kind);
                writeln!(
                    writer,
                    "    \"{}\" -> \"{}\" [label=\"{}\"];",
                    id_from, id_to, edge.short_alias
                )?;
            }
        }
    }
    writeln!(writer, "}}")?;
    Ok(())
}

impl BuildPlanNode {
    fn gen_node_id(&self) -> String {
        match self {
            BuildPlanNode::Check(target) => format!("{:?}@Check", target),
            BuildPlanNode::EmitProof(target) => format!("{:?}@EmitProof", target),
            BuildPlanNode::Prove(target) => format!("{:?}@Prove", target),
            BuildPlanNode::BuildCore(target) => format!("{:?}@BuildCore", target),
            BuildPlanNode::BuildCStub(target, index) => {
                format!("{:?}@BuildCStub_{}", target, index)
            }
            BuildPlanNode::ArchiveOrLinkCStubs(target) => format!("{:?}@ArchiveCStubs", target),
            BuildPlanNode::LinkCore(target) => format!("{:?}@LinkCore", target),
            BuildPlanNode::MakeExecutable(target) => format!("{:?}@MakeExecutable", target),
            BuildPlanNode::GenerateDsym(target) => format!("{:?}@GenerateDsym", target),
            BuildPlanNode::GenerateTestInfo(target) => format!("{:?}@GenerateTestInfo", target),
            BuildPlanNode::GenerateNodeTestPackageConfig(package) => {
                format!("{:?}@GenerateNodeTestPackageConfig", package)
            }
            BuildPlanNode::Bundle(module_id) => format!("{:?}@Bundle", module_id),
            BuildPlanNode::GenerateMbti(target) => format!("{:?}@GenerateMbti", target),
            BuildPlanNode::BuildRuntimeObject(index) => {
                format!("BuildRuntimeObject_{index}")
            }
            BuildPlanNode::BuildRuntimeLib => "BuildRuntimeLib".to_string(),
            BuildPlanNode::BuildDocs(module_id) => format!("{:?}@BuildDocs", module_id),
            BuildPlanNode::BuildVirtual(target) => format!("{:?}@BuildVirtual", target),
        }
    }

    fn gen_label(&self, env: &ResolvedEnv, packages: &DiscoverResult) -> String {
        match self {
            BuildPlanNode::Check(target) => {
                let fqn = packages.fqn(target.package);
                format!("{}\\nCheck", fqn)
            }
            BuildPlanNode::EmitProof(target) => {
                let fqn = packages.fqn(target.package);
                format!("{}\\nEmitProof", fqn)
            }
            BuildPlanNode::Prove(target) => {
                let fqn = packages.fqn(target.package);
                format!("{}\\nProve", fqn)
            }
            BuildPlanNode::BuildCore(target) => {
                let fqn = packages.fqn(target.package);
                format!("{}\\nBuildCore", fqn)
            }
            BuildPlanNode::BuildCStub(target, index) => {
                let fqn = packages.fqn(*target);
                format!("{}\\nBuildCStub_{}", fqn, index)
            }
            BuildPlanNode::ArchiveOrLinkCStubs(target) => {
                let fqn = packages.fqn(*target);
                format!("{}\\nBuildCStubs", fqn)
            }
            BuildPlanNode::LinkCore(target) => {
                let fqn = packages.fqn(target.package);
                format!("{}\\nLinkCore", fqn)
            }
            BuildPlanNode::MakeExecutable(target) => {
                let fqn = packages.fqn(target.package);
                format!("{}\\nMakeExecutable", fqn)
            }
            BuildPlanNode::GenerateDsym(target) => {
                let fqn = packages.fqn(target.package);
                format!("{}\\nGenerateDsym", fqn)
            }
            BuildPlanNode::GenerateTestInfo(target) => {
                let fqn = packages.fqn(target.package);
                format!("{}\\nGenerateTestInfo", fqn)
            }
            BuildPlanNode::GenerateNodeTestPackageConfig(package) => {
                let fqn = packages.fqn(*package);
                format!("{}\\nGenerateNodeTestPackageConfig", fqn)
            }
            BuildPlanNode::Bundle(module_id) => {
                let src = env.module_source(*module_id);
                format!("{}\\nBundle", src)
            }
            BuildPlanNode::GenerateMbti(build_target) => {
                let fqn = packages.fqn(build_target.package);
                format!("{}\\nGenerateMbti", fqn)
            }
            BuildPlanNode::BuildRuntimeObject(index) => {
                format!("BuildRuntimeObject_{index}")
            }
            BuildPlanNode::BuildRuntimeLib => "BuildRuntimeLib".to_string(),
            BuildPlanNode::BuildDocs(module_id) => {
                let src = env.module_source(*module_id);
                format!("{}\\nBuildDocs", src)
            }
            BuildPlanNode::BuildVirtual(package) => {
                let fqn = packages.fqn(*package);
                format!("{}\\nBuildVirtual", fqn)
            }
        }
    }

    fn gen_color(&self) -> &'static str {
        match self {
            BuildPlanNode::Check(_) => "lightblue",
            BuildPlanNode::EmitProof(_) => "lightgoldenrod2",
            BuildPlanNode::Prove(_) => "lightgoldenrod1",
            BuildPlanNode::BuildCore(_) => "lightgreen",
            BuildPlanNode::BuildCStub(_, _) => "lightsalmon",
            BuildPlanNode::ArchiveOrLinkCStubs(_) => "lightyellow",
            BuildPlanNode::LinkCore(_) => "lightcoral",
            BuildPlanNode::MakeExecutable(_) => "lightpink",
            BuildPlanNode::GenerateDsym(_) => "pink",
            BuildPlanNode::GenerateTestInfo(_) => "lightgray",
            BuildPlanNode::GenerateNodeTestPackageConfig(_) => "lightgray",
            BuildPlanNode::Bundle(_) => "wheat",
            BuildPlanNode::GenerateMbti(_) => "lightcyan",
            BuildPlanNode::BuildRuntimeObject(_) => "orange",
            BuildPlanNode::BuildRuntimeLib => "orange",
            BuildPlanNode::BuildDocs(_) => "lavender",
            BuildPlanNode::BuildVirtual(_) => "lightsteelblue",
        }
    }
}

impl BuildPlanActionKey {
    fn gen_node_id(&self, packages: &DiscoverResult) -> String {
        match self {
            Self::Backend(node) => node.gen_node_id(),
            Self::PackagePrebuild(PackagePrebuildKey::Custom {
                package,
                declaration_index,
            }) => format!("{package:?}@RunPrebuild_{declaration_index}"),
            Self::PackagePrebuild(PackagePrebuildKey::MoonLex { package, input }) => {
                let package_root = &packages.get_package(*package).root_path;
                let input = package_file_key(package_root, input);
                format!("{package:?}@RunMoonLexPrebuild_{}", input.display())
            }
            Self::PackagePrebuild(PackagePrebuildKey::MoonYacc { package, input }) => {
                let package_root = &packages.get_package(*package).root_path;
                let input = package_file_key(package_root, input);
                format!("{package:?}@RunMoonYaccPrebuild_{}", input.display())
            }
        }
    }

    fn gen_label(&self, env: &ResolvedEnv, packages: &DiscoverResult) -> String {
        let file_name = |path: &Path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string())
        };
        match self {
            Self::Backend(node) => node.gen_label(env, packages),
            Self::PackagePrebuild(PackagePrebuildKey::Custom {
                package,
                declaration_index,
            }) => format!(
                "{}\\nRunPrebuild_{}",
                packages.fqn(*package),
                declaration_index
            ),
            Self::PackagePrebuild(PackagePrebuildKey::MoonLex { package, input }) => {
                format!(
                    "{}\\nRunMoonLexPrebuild_{}",
                    packages.fqn(*package),
                    file_name(input)
                )
            }
            Self::PackagePrebuild(PackagePrebuildKey::MoonYacc { package, input }) => {
                format!(
                    "{}\\nRunMoonYaccPrebuild_{}",
                    packages.fqn(*package),
                    file_name(input)
                )
            }
        }
    }

    fn gen_color(&self) -> &'static str {
        match self {
            Self::Backend(node) => node.gen_color(),
            Self::PackagePrebuild(PackagePrebuildKey::Custom { .. }) => "khaki",
            Self::PackagePrebuild(PackagePrebuildKey::MoonLex { .. }) => "plum",
            Self::PackagePrebuild(PackagePrebuildKey::MoonYacc { .. }) => "thistle",
        }
    }
}

/// Print a build plan as a DOT graph, showing build nodes and their dependencies
pub fn print_build_plan_dot(
    build_plan: &BuildPlan,
    env: &ResolvedEnv,
    packages: &DiscoverResult,
    writer: &mut dyn Write,
) -> io::Result<()> {
    writeln!(writer, "digraph BuildPlan {{")?;
    writeln!(writer, "    rankdir=TB;")?;
    writeln!(writer, "    node [shape=box];")?;

    // Nodes: label both backend and package-prebuild actions.
    for action in build_plan.all_actions() {
        let node_id = action.gen_node_id(packages);
        let label = action.gen_label(env, packages);
        let color = action.gen_color();

        writeln!(
            writer,
            "    \"{}\" [label=\"{}\" fillcolor=\"{}\" style=\"filled\"];",
            node_id, label, color
        )?;
    }

    // Edges: artifact requirements between Build Plan actions.
    for action in build_plan.all_actions() {
        for dep in build_plan.dependency_actions(&action) {
            let node_id = action.gen_node_id(packages);
            let dep_id = dep.gen_node_id(packages);
            writeln!(writer, "    \"{}\" -> \"{}\";\n", node_id, dep_id)?;
        }
    }

    writeln!(writer, "}}")?;
    Ok(())
}
