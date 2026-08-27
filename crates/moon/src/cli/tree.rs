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

use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::Hash;
use std::path::PathBuf;

use anyhow::Context;
use moonbuild_rupes_recta::{
    discover::DiscoverResult,
    model::{BuildTarget, PackageId, TargetKind},
    resolve::{ResolveConfig, ResolveOutput, resolve_synced_project, sync_dependencies},
};
use mooncake::pkg::{sync::SyncOutputOptions, tree::ResolvedTree};
use moonutil::{
    child_process::ChildOutputMode,
    cli_support::UniversalFlags,
    command_output::CommandOutput,
    manifest::read_module_desc_file_in_dir,
    project::PackageDirs,
    resolution::{DependencyKind, ModuleId, ModuleName, ModuleSourceKind, ResolvedEnv},
    user_log::{UserLog, UserLogCapture, UserLogEntry},
};
use serde::Serialize;

use super::{
    invocation::{JsonCommand, JsonCommandOutcome},
    require_selected_module,
};

const MODULE_TREE_JSON_VERSION: u32 = 1;
const PACKAGE_TREE_JSON_VERSION: u32 = 2;
const TREE_JSON_ERROR_EXIT_CODE: i32 = -1;

/// Display the dependency tree
///
/// Text output expands each dependency once per root. When another path
/// reaches a dependency whose children were already displayed, the dependency
/// remains visible with `(*)`, but its children are omitted. Use
/// `--no-dedupe` to repeat those subgraphs. Cycles remain marked as `(cycle)`.
/// JSON output represents shared dependencies once and preserves their
/// relationships as edges.
#[derive(Debug, clap::Parser)]
pub(crate) struct TreeSubcommand {
    /// Output one complete JSON result to stdout
    #[clap(long)]
    pub json: bool,

    /// Repeat dependency subgraphs instead of marking them with `(*)`
    #[clap(long, conflicts_with = "json")]
    pub no_dedupe: bool,

    /// Show the package-level dependency graph instead of the module-level tree
    ///
    /// Text output expands source imports from every package in the selected
    /// module. With `--json`, the result contains every non-standard-library
    /// package in the resolved project, all import target kinds, and the
    /// selected module's packages in `root`.
    #[clap(long)]
    pub package: bool,
}

pub(crate) fn tree_cli(
    cli: UniversalFlags,
    cmd: TreeSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let dedupe = !cmd.no_dedupe;
    let rendered = if cmd.package {
        let (resolve_output, selected) =
            resolve_selected_package_graph(&cli, output.user_log(), ChildOutputMode::Inherit)?;
        render_package_tree(&resolve_output, selected, dedupe)
    } else {
        let resolved = resolve_selected_tree(&cli, output.user_log())?;
        let workspace_members =
            (resolved.workspace_members.len() > 1).then_some(&resolved.workspace_members);
        render_tree(&resolved.env, resolved.root, workspace_members, dedupe)
    };
    output.write_result(|writer| -> anyhow::Result<()> {
        writer.write_all(rendered.as_bytes())?;
        Ok(())
    })?;
    Ok(0)
}

fn selected_tree_project(
    cli: &UniversalFlags,
    user_log: &UserLog,
) -> anyhow::Result<(PathBuf, PackageDirs)> {
    let project = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .select(user_log)?;
    let module_dir = require_selected_module(project.context(), "tree")?;
    let dirs = project.package_dirs()?;
    Ok((module_dir, dirs))
}

fn resolve_selected_tree(cli: &UniversalFlags, user_log: &UserLog) -> anyhow::Result<ResolvedTree> {
    let (module_dir, dirs) = selected_tree_project(cli, user_log)?;
    mooncake::pkg::tree::tree(&module_dir, &dirs.project_manifest, user_log)
}

/// Resolve the package-level dependency graph through the Rupes Recta
/// pipeline, together with the module the tree is rooted at.
fn resolve_selected_package_graph(
    cli: &UniversalFlags,
    user_log: &UserLog,
    child_output: ChildOutputMode,
) -> anyhow::Result<(ResolveOutput, ModuleId)> {
    let (module_dir, dirs) = selected_tree_project(cli, user_log)?;
    let resolve_cfg =
        ResolveConfig::new_with_load_defaults(false, false, false, cli.workspace_env.clone())
            .with_sync_output(SyncOutputOptions {
                quiet: false,
                child_output,
            });
    let synced = sync_dependencies(&resolve_cfg, &dirs, user_log)?;
    let resolve_output = resolve_synced_project(&resolve_cfg, synced, user_log)?;

    let module = read_module_desc_file_in_dir(&module_dir)?;
    let module_name: ModuleName = module.name.as_str().into();
    let selected = resolve_output
        .local_modules()
        .iter()
        .copied()
        .find(|id| resolve_output.module_rel.module_source(*id).name() == &module_name)
        .or_else(|| resolve_output.local_modules().first().copied())
        .context("resolved dependency graph has no root modules")?;
    Ok((resolve_output, selected))
}

fn run_tree_json(
    cli: &UniversalFlags,
    cmd: &TreeSubcommand,
    output: &CommandOutput,
) -> TreeJsonOutcome {
    let user_log = output.user_log();
    if cmd.package {
        TreeJsonOutcome::package(
            resolve_selected_package_graph(cli, user_log, ChildOutputMode::Capture)
                .map(|(resolved, selected)| (Box::new(resolved), selected))
                .map_err(|error| format!("{error:#}")),
        )
    } else {
        TreeJsonOutcome::module(
            resolve_selected_tree(cli, user_log)
                .map(Box::new)
                .map_err(|error| format!("{error:#}")),
        )
    }
}

struct TreeJsonOutcome {
    exit_code: i32,
    kind: TreeJsonOutcomeKind,
}

enum TreeJsonOutcomeKind {
    Module(Result<Box<ResolvedTree>, String>),
    Package(Result<(Box<ResolveOutput>, ModuleId), String>),
}

impl TreeJsonOutcome {
    fn module(result: Result<Box<ResolvedTree>, String>) -> Self {
        Self {
            exit_code: if result.is_ok() {
                0
            } else {
                TREE_JSON_ERROR_EXIT_CODE
            },
            kind: TreeJsonOutcomeKind::Module(result),
        }
    }

    fn package(result: Result<(Box<ResolveOutput>, ModuleId), String>) -> Self {
        Self {
            exit_code: if result.is_ok() {
                0
            } else {
                TREE_JSON_ERROR_EXIT_CODE
            },
            kind: TreeJsonOutcomeKind::Package(result),
        }
    }

    fn exit_code(&self) -> i32 {
        self.exit_code
    }
}

#[derive(Debug)]
struct TreeJsonCommand {
    command: TreeSubcommand,
}

pub(crate) fn json_command(command: TreeSubcommand) -> Box<dyn JsonCommand> {
    Box::new(TreeJsonCommand { command })
}

impl JsonCommand for TreeJsonCommand {
    fn run(&self, flags: &UniversalFlags, output: &CommandOutput) -> JsonCommandOutcome {
        tree_json_outcome(run_tree_json(flags, &self.command, output))
    }

    fn bootstrap_error(&self, message: String) -> JsonCommandOutcome {
        let outcome = if self.command.package {
            TreeJsonOutcome::package(Err(message))
        } else {
            TreeJsonOutcome::module(Err(message))
        };
        tree_json_outcome(outcome)
    }
}

fn tree_json_outcome(outcome: TreeJsonOutcome) -> JsonCommandOutcome {
    let exit_code = outcome.exit_code();
    JsonCommandOutcome::new(exit_code, move |output, capture| {
        write_tree_json(output, capture, outcome)
    })
}

fn write_tree_json(
    output: &CommandOutput,
    capture: &UserLogCapture,
    outcome: TreeJsonOutcome,
) -> anyhow::Result<()> {
    let status = if outcome.exit_code() == 0 {
        "success"
    } else {
        "failure"
    };
    let logs = capture.take();
    output.write_result(|writer| -> anyhow::Result<()> {
        match outcome.kind {
            TreeJsonOutcomeKind::Module(Ok(resolved)) => {
                let graph =
                    render_graph_json(&resolved.env, resolved.root, &resolved.workspace_members);
                let report = TreeJsonReport {
                    version: MODULE_TREE_JSON_VERSION,
                    status,
                    error: None,
                    root: Some(graph.root),
                    modules: graph.modules,
                    edges: graph.edges,
                    logs,
                };
                serde_json::to_writer(&mut *writer, &report)?;
            }
            TreeJsonOutcomeKind::Package(Ok((resolve_output, selected))) => {
                let report = render_package_json_report(&resolve_output, selected, status, logs);
                serde_json::to_writer(&mut *writer, &report)?;
            }
            TreeJsonOutcomeKind::Package(Err(error)) => {
                let report = PackageJsonReport {
                    version: PACKAGE_TREE_JSON_VERSION,
                    status,
                    error: Some(error),
                    root: vec![],
                    nodes: vec![],
                    edges: vec![],
                    logs,
                };
                serde_json::to_writer(&mut *writer, &report)?;
            }
            TreeJsonOutcomeKind::Module(Err(error)) => {
                let report = TreeJsonReport {
                    version: MODULE_TREE_JSON_VERSION,
                    status,
                    error: Some(error),
                    root: None,
                    modules: vec![],
                    edges: vec![],
                    logs,
                };
                serde_json::to_writer(&mut *writer, &report)?;
            }
        }
        writeln!(writer)?;
        Ok(())
    })
}

#[derive(Debug, Serialize)]
struct TreeJsonReport {
    version: u32,
    status: &'static str,
    error: Option<String>,
    root: Option<usize>,
    modules: Vec<ModuleJSON>,
    edges: Vec<EdgeJSON>,
    logs: Vec<UserLogEntry>,
}

#[derive(Debug, Serialize)]
struct TreeGraphJSON {
    root: usize,
    modules: Vec<ModuleJSON>,
    edges: Vec<EdgeJSON>,
}

#[derive(Debug, Serialize)]
struct ModuleJSON {
    name: String,
    version: String,
    source: SourceJSON,
    workspace_member: bool,
}

#[derive(Debug, Serialize)]
struct SourceJSON {
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
}

#[derive(Debug, Serialize)]
struct EdgeJSON {
    from: usize,
    to: usize,
    name: String,
    kind: &'static str,
}

#[derive(Debug, Serialize)]
struct PackageJsonReport {
    version: u32,
    status: &'static str,
    error: Option<String>,
    root: Vec<usize>,
    nodes: Vec<PackageNodeJSON>,
    edges: Vec<PackageEdgeJSON>,
    logs: Vec<UserLogEntry>,
}

#[derive(Debug, Serialize)]
struct PackageGraphJSON {
    root: Vec<usize>,
    nodes: Vec<PackageNodeJSON>,
    edges: Vec<PackageEdgeJSON>,
}

#[derive(Debug, Serialize)]
struct PackageNodeJSON {
    module: String,
    version: String,
    source: SourceJSON,
    rel: String,
}

#[derive(Debug, Serialize)]
struct PackageEdgeJSON {
    from: usize,
    to: usize,
    alias: String,
    kinds: Vec<&'static str>,
}

fn render_graph_json(
    resolved: &ResolvedEnv,
    root: ModuleId,
    workspace_members: &HashSet<ModuleId>,
) -> TreeGraphJSON {
    let mut modules = resolved
        .all_modules_and_id()
        .map(|(id, source)| {
            (
                id,
                ModuleJSON {
                    name: source.name().to_string(),
                    version: source.version().to_string(),
                    source: source_json(source.source()),
                    workspace_member: workspace_members.contains(&id),
                },
            )
        })
        .collect::<Vec<_>>();
    modules.sort_by(|(lhs_id, lhs), (rhs_id, rhs)| {
        (lhs.name.as_str(), lhs.version.as_str())
            .cmp(&(rhs.name.as_str(), rhs.version.as_str()))
            .then_with(|| {
                resolved
                    .module_source(*lhs_id)
                    .cmp(resolved.module_source(*rhs_id))
            })
    });
    let index = modules
        .iter()
        .enumerate()
        .map(|(index, (id, _))| (*id, index))
        .collect::<HashMap<_, _>>();

    let mut edges = Vec::new();
    for (source_id, _) in &modules {
        let mut deps = resolved.deps_keyed(*source_id).collect::<Vec<_>>();
        deps.sort_by(|(lhs_id, lhs_edge), (rhs_id, rhs_edge)| {
            lhs_edge
                .name
                .cmp(&rhs_edge.name)
                .then_with(|| index[lhs_id].cmp(&index[rhs_id]))
        });
        for (dep_id, dep_edge) in deps {
            edges.push(EdgeJSON {
                from: index[source_id],
                to: index[&dep_id],
                name: dep_edge.name.to_string(),
                kind: match dep_edge.kind {
                    DependencyKind::Regular => "regular",
                    DependencyKind::Binary => "binary",
                },
            });
        }
    }

    TreeGraphJSON {
        root: index[&root],
        modules: modules.into_iter().map(|(_, module)| module).collect(),
        edges,
    }
}

fn render_package_json_report(
    resolve_output: &ResolveOutput,
    selected_module: ModuleId,
    status: &'static str,
    logs: Vec<UserLogEntry>,
) -> PackageJsonReport {
    let graph = render_package_graph_json(resolve_output, selected_module);
    PackageJsonReport {
        version: PACKAGE_TREE_JSON_VERSION,
        status,
        error: None,
        root: graph.root,
        nodes: graph.nodes,
        edges: graph.edges,
        logs,
    }
}

/// Render the package-level dependency graph as JSON.
///
/// Standard-library packages are excluded from both nodes and edges, so the
/// output focuses on the project and its external dependencies, mirroring the
/// module-level view (`moon tree --json`).
fn render_package_graph_json(
    resolve_output: &ResolveOutput,
    selected_module: ModuleId,
) -> PackageGraphJSON {
    let pkg_dirs = &resolve_output.pkg_dirs;
    let dep_graph = &resolve_output.pkg_rel.dep_graph;

    // Package nodes are deduplicated by PackageId. Build the set from discovered
    // packages so packages with no non-stdlib edges remain visible as isolated nodes.
    let package_ids = pkg_dirs
        .all_packages(true)
        .map(|(package, _)| package)
        .collect::<HashSet<_>>();

    let mut nodes = package_ids
        .into_iter()
        .map(|package| {
            let fqn = pkg_dirs.fqn(package);
            let module_source = fqn.module();
            (
                package,
                PackageNodeJSON {
                    module: module_source.name().to_string(),
                    version: module_source.version().to_string(),
                    source: source_json(module_source.source()),
                    rel: fqn.package().to_string(),
                },
            )
        })
        .collect::<Vec<_>>();
    nodes.sort_by(|(lhs_package, lhs), (rhs_package, rhs)| {
        (lhs.module.as_str(), lhs.version.as_str(), lhs.rel.as_str())
            .cmp(&(rhs.module.as_str(), rhs.version.as_str(), rhs.rel.as_str()))
            .then_with(|| lhs_package.cmp(rhs_package))
    });
    let index = nodes
        .iter()
        .enumerate()
        .map(|(index, (package, _))| (*package, index))
        .collect::<HashMap<_, _>>();

    // Package edges are deduplicated by package pair and alias. Keep all target
    // kinds as provenance so projection does not lose dependency information.
    let mut edge_kinds = HashMap::<(PackageId, PackageId, String), BTreeSet<_>>::new();
    for (from, to, edge) in dep_graph.all_edges() {
        if from.package == to.package
            || !index.contains_key(&from.package)
            || !index.contains_key(&to.package)
        {
            continue;
        }
        edge_kinds
            .entry((from.package, to.package, edge.short_alias.to_string()))
            .or_default()
            .insert(target_kind_str(edge.kind));
    }
    let mut edges = edge_kinds
        .into_iter()
        .map(|((from, to, alias), kinds)| PackageEdgeJSON {
            from: index[&from],
            to: index[&to],
            alias,
            kinds: kinds.into_iter().collect(),
        })
        .collect::<Vec<_>>();
    edges.sort_by(|lhs, rhs| {
        (lhs.from, lhs.alias.as_str(), lhs.to).cmp(&(rhs.from, rhs.alias.as_str(), rhs.to))
    });

    let root = selected_source_packages(resolve_output, selected_module)
        .iter()
        .filter_map(|package| index.get(package))
        .copied()
        .collect::<Vec<_>>();

    PackageGraphJSON {
        root,
        nodes: nodes.into_iter().map(|(_, node)| node).collect(),
        edges,
    }
}

fn target_kind_str(kind: TargetKind) -> &'static str {
    match kind {
        TargetKind::Source => "source",
        TargetKind::WhiteboxTest => "whitebox-test",
        TargetKind::BlackboxTest => "blackbox-test",
        TargetKind::InlineTest => "inline-test",
        TargetKind::SubPackage => "sub-package",
    }
}

/// Render the package-level dependency graph as a text tree, rooted at the
/// source packages of the selected module.
///
/// Source targets form the roots; expansion follows all non-stdlib dependency
/// edges, mirroring the module-level text tree's treatment of stdlib modules.
fn render_package_tree(
    resolve_output: &ResolveOutput,
    selected_module: ModuleId,
    dedupe: bool,
) -> String {
    let pkg_dirs = &resolve_output.pkg_dirs;
    let source_packages = selected_source_packages(resolve_output, selected_module);
    let sorted_children = |source| sorted_package_tree_children(resolve_output, source);

    let mut out = String::new();
    for (root_idx, package_id) in source_packages.iter().enumerate() {
        if root_idx > 0 {
            out.push('\n');
        }
        let target = package_id.build_target(TargetKind::Source);
        out.push_str(&format_package_label(pkg_dirs, target));
        out.push_str(":\n");

        let mut stack = HashSet::new();
        stack.insert(target);
        let mut expanded = HashSet::new();
        expanded.insert(target);
        let direct_dep_count = render_tree_children(
            sorted_children(target),
            "",
            &mut stack,
            &mut expanded,
            dedupe,
            &mut out,
            &sorted_children,
        );
        if direct_dep_count == 0 {
            out.push_str("  (no dependencies)\n");
        }
    }
    out
}

fn selected_source_packages(resolve_output: &ResolveOutput, module: ModuleId) -> Vec<PackageId> {
    resolve_output
        .pkg_dirs
        .packages_for_module(module)
        .into_iter()
        .flat_map(|packages| packages.values().copied())
        .collect()
}

fn sorted_package_tree_children(
    resolve_output: &ResolveOutput,
    source: BuildTarget,
) -> Vec<TreeChild<BuildTarget>> {
    let pkg_dirs = &resolve_output.pkg_dirs;
    let dep_graph = &resolve_output.pkg_rel.dep_graph;

    let mut deps = dep_graph
        .edges_directed(source, petgraph::Direction::Outgoing)
        .filter(|(_, to, _)| !pkg_dirs.is_stdlib_package(to.package))
        .map(|(_, to, edge)| {
            (
                to,
                edge.short_alias.to_string(),
                format_package_label(pkg_dirs, to),
            )
        })
        .collect::<Vec<_>>();
    deps.sort_by(
        |(lhs_to, lhs_alias, lhs_label), (rhs_to, rhs_alias, rhs_label)| {
            lhs_alias
                .cmp(rhs_alias)
                .then_with(|| lhs_label.cmp(rhs_label))
                .then_with(|| lhs_to.cmp(rhs_to))
        },
    );

    deps.into_iter()
        .map(|(node, alias, label)| TreeChild {
            node,
            label: format!("{alias} -> {label}"),
        })
        .collect()
}

fn format_package_label(pkg_dirs: &DiscoverResult, target: BuildTarget) -> String {
    let fqn = pkg_dirs.fqn(target.package);
    let module_source = fqn.module();
    let mut label = module_source.to_string();
    let rel = fqn.package().to_string();
    if !rel.is_empty() {
        label.push_str(&format!(" [{rel}]"));
    }
    label
}

fn source_json(source: &ModuleSourceKind) -> SourceJSON {
    match source {
        ModuleSourceKind::Registry => SourceJSON {
            kind: "registry",
            path: None,
            url: None,
        },
        ModuleSourceKind::Local(path) => SourceJSON {
            kind: "local",
            path: Some(path.display().to_string()),
            url: None,
        },
        ModuleSourceKind::Git(url) => SourceJSON {
            kind: "git",
            path: None,
            url: Some(url.clone()),
        },
        ModuleSourceKind::Stdlib(path) => SourceJSON {
            kind: "stdlib",
            path: Some(path.display().to_string()),
            url: None,
        },
        ModuleSourceKind::SingleFile(path) => SourceJSON {
            kind: "single-file",
            path: Some(path.display().to_string()),
            url: None,
        },
    }
}

fn render_tree(
    resolved: &ResolvedEnv,
    root: ModuleId,
    workspace_members: Option<&HashSet<ModuleId>>,
    dedupe: bool,
) -> String {
    let sorted_children = |source| sorted_module_tree_children(resolved, source, workspace_members);
    let mut out = String::new();
    out.push_str(&format_module_label(resolved, root, workspace_members));
    out.push_str(":\n");

    let mut stack = HashSet::new();
    stack.insert(root);
    let mut expanded = HashSet::new();
    expanded.insert(root);
    let direct_dep_count = render_tree_children(
        sorted_children(root),
        "",
        &mut stack,
        &mut expanded,
        dedupe,
        &mut out,
        &sorted_children,
    );

    if direct_dep_count == 0 {
        out.push_str("  (no dependencies)\n");
    }

    out
}

fn sorted_module_tree_children(
    resolved: &ResolvedEnv,
    source: ModuleId,
    workspace_members: Option<&HashSet<ModuleId>>,
) -> Vec<TreeChild<ModuleId>> {
    let mut deps = resolved.deps_keyed(source).collect::<Vec<_>>();
    deps.sort_by(|(lhs_id, lhs_edge), (rhs_id, rhs_edge)| {
        lhs_edge.name.cmp(&rhs_edge.name).then_with(|| {
            resolved
                .module_source(*lhs_id)
                .cmp(resolved.module_source(*rhs_id))
        })
    });

    deps.into_iter()
        .map(|(node, edge)| TreeChild {
            node,
            label: format!(
                "{} -> {}",
                edge.name,
                format_module_label(resolved, node, workspace_members)
            ),
        })
        .collect()
}

struct TreeChild<Node> {
    node: Node,
    label: String,
}

fn render_tree_children<Node, Children>(
    children: Vec<TreeChild<Node>>,
    indent: &str,
    stack: &mut HashSet<Node>,
    expanded: &mut HashSet<Node>,
    dedupe: bool,
    out: &mut String,
    sorted_children: &Children,
) -> usize
where
    Node: Copy + Eq + Hash,
    Children: Fn(Node) -> Vec<TreeChild<Node>>,
{
    for (idx, child) in children.iter().enumerate() {
        let is_last = idx + 1 == children.len();
        let is_cycle = stack.contains(&child.node);
        let already_expanded = !expanded.insert(child.node);
        let descendants = if is_cycle {
            Vec::new()
        } else {
            sorted_children(child.node)
        };
        let omit_descendants = dedupe && already_expanded;

        out.push_str(indent);
        out.push_str(if is_last { "└─" } else { "├─" });
        out.push(' ');
        out.push_str(&child.label);
        if omit_descendants && !descendants.is_empty() {
            out.push_str(" (*)");
        }
        out.push('\n');

        let next_indent = format!("{indent}{}", if is_last { "   " } else { "│  " });
        if is_cycle {
            out.push_str(&next_indent);
            out.push_str("└─ (cycle)\n");
            continue;
        }

        if omit_descendants {
            continue;
        }

        stack.insert(child.node);
        render_tree_children(
            descendants,
            &next_indent,
            stack,
            expanded,
            dedupe,
            out,
            sorted_children,
        );
        stack.remove(&child.node);
    }

    children.len()
}

fn format_module_label(
    resolved: &ResolvedEnv,
    id: ModuleId,
    workspace_members: Option<&HashSet<ModuleId>>,
) -> String {
    let mut label = resolved.module_source(id).to_string();
    if workspace_members.is_some_and(|members| members.contains(&id)) {
        label.push_str(" [workspace member]");
    }
    label
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;
    use moonutil::manifest::MoonMod;
    use moonutil::resolution::{
        DependencyEdge, DependencyKind, ModuleSource, ModuleSourceKind, ResolvedModule,
    };
    use std::sync::Arc;

    fn local_source(name: &str, version: &str, path: &str) -> ModuleSource {
        ModuleSource::new_full(
            name.parse().unwrap(),
            version.parse().unwrap(),
            ModuleSourceKind::Local(path.into()),
        )
    }

    fn local_module(name: &str, version: &str) -> Arc<MoonMod> {
        Arc::new(MoonMod {
            name: name.to_string(),
            version: Some(version.parse().unwrap()),
            ..Default::default()
        })
    }

    fn regular_dep(name: &str) -> DependencyEdge {
        DependencyEdge {
            name: name.parse().unwrap(),
            kind: DependencyKind::Regular,
        }
    }

    #[test]
    fn no_dedupe_is_text_only() {
        let command =
            <TreeSubcommand as clap::Parser>::try_parse_from(["tree", "--no-dedupe"]).unwrap();
        assert!(command.no_dedupe);
        assert!(
            <TreeSubcommand as clap::Parser>::try_parse_from(["tree", "--json", "--no-dedupe"])
                .is_err()
        );
    }

    fn shared_subgraph() -> (ResolvedEnv, ModuleId) {
        let (roots, root) = ResolvedModule::only_one_module(
            local_source("alice/root", "0.1.0", "/workspace/root"),
            local_module("alice/root", "0.1.0"),
        );
        let mut env = ResolvedEnv::from_root_modules(roots);
        let dep_a = env.add_module(
            local_source("alice/a", "0.1.0", "/workspace/a"),
            local_module("alice/a", "0.1.0"),
        );
        let dep_b = env.add_module(
            local_source("alice/b", "0.1.0", "/workspace/b"),
            local_module("alice/b", "0.1.0"),
        );
        let shared = env.add_module(
            local_source("alice/shared", "0.1.0", "/workspace/shared"),
            local_module("alice/shared", "0.1.0"),
        );
        let leaf = env.add_module(
            local_source("alice/leaf", "0.1.0", "/workspace/leaf"),
            local_module("alice/leaf", "0.1.0"),
        );

        env.add_dependency(root, dep_a, &regular_dep("alice/a"));
        env.add_dependency(root, dep_b, &regular_dep("alice/b"));
        env.add_dependency(dep_a, shared, &regular_dep("alice/shared"));
        env.add_dependency(dep_b, shared, &regular_dep("alice/shared"));
        env.add_dependency(shared, leaf, &regular_dep("alice/leaf"));
        env.add_dependency(dep_b, leaf, &regular_dep("alice/leaf"));
        (env, root)
    }

    #[test]
    fn tree_render_uses_three_column_unicode_indent() {
        let (roots, root_id) = ResolvedModule::only_one_module(
            local_source("alice/root", "0.1.0", "/workspace/root"),
            local_module("alice/root", "0.1.0"),
        );
        let mut env = ResolvedEnv::from_root_modules(roots);

        let dep_a = env.add_module(
            local_source("alice/a", "0.1.0", "/workspace/a"),
            local_module("alice/a", "0.1.0"),
        );
        let dep_b = env.add_module(
            local_source("alice/b", "0.1.0", "/workspace/b"),
            local_module("alice/b", "0.1.0"),
        );
        let dep_c = env.add_module(
            local_source("alice/c", "0.1.0", "/workspace/c"),
            local_module("alice/c", "0.1.0"),
        );

        env.add_dependency(root_id, dep_a, &regular_dep("alice/a"));
        env.add_dependency(root_id, dep_b, &regular_dep("alice/b"));
        env.add_dependency(dep_a, dep_c, &regular_dep("alice/c"));

        let rendered = render_tree(&env, root_id, None, true);
        expect![[r#"
            alice/root@0.1.0 (local /workspace/root):
            ├─ alice/a -> alice/a@0.1.0 (local /workspace/a)
            │  └─ alice/c -> alice/c@0.1.0 (local /workspace/c)
            └─ alice/b -> alice/b@0.1.0 (local /workspace/b)
        "#]]
        .assert_eq(&rendered);
    }

    #[test]
    fn tree_render_includes_local_dependency_source() {
        let (roots, root_id) = ResolvedModule::only_one_module(
            local_source("username/hello", "0.1.0", "/workspace/hello"),
            local_module("username/hello", "0.1.0"),
        );
        let mut env = ResolvedEnv::from_root_modules(roots);
        let dep_id = env.add_module(
            local_source("just/hello004", "0.1.0", "/workspace/hello/deps/hello004"),
            local_module("just/hello004", "0.1.0"),
        );
        env.add_dependency(root_id, dep_id, &regular_dep("just/hello004"));

        let rendered = render_tree(&env, root_id, None, true);
        expect![[r#"
            username/hello@0.1.0 (local /workspace/hello):
            └─ just/hello004 -> just/hello004@0.1.0 (local /workspace/hello/deps/hello004)
        "#]]
        .assert_eq(&rendered);
    }

    #[test]
    fn tree_render_expands_shared_subgraph_once() {
        let (env, root) = shared_subgraph();

        let rendered = render_tree(&env, root, None, true);
        expect![[r#"
            alice/root@0.1.0 (local /workspace/root):
            ├─ alice/a -> alice/a@0.1.0 (local /workspace/a)
            │  └─ alice/shared -> alice/shared@0.1.0 (local /workspace/shared)
            │     └─ alice/leaf -> alice/leaf@0.1.0 (local /workspace/leaf)
            └─ alice/b -> alice/b@0.1.0 (local /workspace/b)
               ├─ alice/leaf -> alice/leaf@0.1.0 (local /workspace/leaf)
               └─ alice/shared -> alice/shared@0.1.0 (local /workspace/shared) (*)
        "#]]
        .assert_eq(&rendered);
    }

    #[test]
    fn tree_render_can_repeat_shared_subgraphs() {
        let (env, root) = shared_subgraph();

        let rendered = render_tree(&env, root, None, false);
        expect![[r#"
            alice/root@0.1.0 (local /workspace/root):
            ├─ alice/a -> alice/a@0.1.0 (local /workspace/a)
            │  └─ alice/shared -> alice/shared@0.1.0 (local /workspace/shared)
            │     └─ alice/leaf -> alice/leaf@0.1.0 (local /workspace/leaf)
            └─ alice/b -> alice/b@0.1.0 (local /workspace/b)
               ├─ alice/leaf -> alice/leaf@0.1.0 (local /workspace/leaf)
               └─ alice/shared -> alice/shared@0.1.0 (local /workspace/shared)
                  └─ alice/leaf -> alice/leaf@0.1.0 (local /workspace/leaf)
        "#]]
        .assert_eq(&rendered);
    }

    #[test]
    fn tree_render_stops_cycles_without_deduplication() {
        let (roots, root) = ResolvedModule::only_one_module(
            local_source("alice/root", "0.1.0", "/workspace/root"),
            local_module("alice/root", "0.1.0"),
        );
        let mut env = ResolvedEnv::from_root_modules(roots);
        let dep = env.add_module(
            local_source("alice/dep", "0.1.0", "/workspace/dep"),
            local_module("alice/dep", "0.1.0"),
        );
        env.add_dependency(root, dep, &regular_dep("alice/dep"));
        env.add_dependency(dep, root, &regular_dep("alice/root"));

        let rendered = render_tree(&env, root, None, false);
        expect![[r#"
            alice/root@0.1.0 (local /workspace/root):
            └─ alice/dep -> alice/dep@0.1.0 (local /workspace/dep)
               └─ alice/root -> alice/root@0.1.0 (local /workspace/root)
                  └─ (cycle)
        "#]]
        .assert_eq(&rendered);
    }

    #[test]
    fn tree_json_represents_shared_subgraph_once() {
        let (env, root) = shared_subgraph();

        let graph = render_graph_json(&env, root, &HashSet::new());
        let shared = graph
            .modules
            .iter()
            .position(|module| module.name == "alice/shared")
            .unwrap();

        assert_eq!(
            graph
                .modules
                .iter()
                .filter(|module| module.name == "alice/shared")
                .count(),
            1
        );
        assert_eq!(
            graph.edges.iter().filter(|edge| edge.to == shared).count(),
            2
        );
    }

    #[test]
    fn tree_render_marks_workspace_members() {
        let mut roots = moonutil::resolution::ResolvedRootModules::with_key();
        let app = roots.insert(ResolvedModule::new(
            local_source("alice/app", "0.1.0", "/workspace/app"),
            local_module("alice/app", "0.1.0"),
        ));
        let liba = roots.insert(ResolvedModule::new(
            local_source("alice/liba", "0.1.1", "/workspace/liba"),
            local_module("alice/liba", "0.1.1"),
        ));
        let mut env = ResolvedEnv::from_root_modules(roots);
        env.add_dependency(app, liba, &regular_dep("alice/liba"));

        let workspace_members = [app, liba].into_iter().collect();
        let rendered = render_tree(&env, app, Some(&workspace_members), true);
        expect![[r#"
            alice/app@0.1.0 (local /workspace/app) [workspace member]:
            └─ alice/liba -> alice/liba@0.1.1 (local /workspace/liba) [workspace member]
        "#]]
        .assert_eq(&rendered);
    }

    #[test]
    fn tree_json_uses_deterministic_module_and_edge_order() {
        let (roots, root_id) = ResolvedModule::only_one_module(
            local_source("alice/root", "0.1.0", "/workspace/root"),
            local_module("alice/root", "0.1.0"),
        );
        let mut env = ResolvedEnv::from_root_modules(roots);

        let dep_a = env.add_module(
            local_source("alice/a", "0.1.0", "/workspace/a"),
            local_module("alice/a", "0.1.0"),
        );
        let dep_b = env.add_module(
            local_source("alice/b", "0.1.0", "/workspace/b"),
            local_module("alice/b", "0.1.0"),
        );
        let dep_c = env.add_module(
            local_source("alice/c", "0.1.0", "/workspace/c"),
            local_module("alice/c", "0.1.0"),
        );

        env.add_dependency(root_id, dep_a, &regular_dep("alice/a"));
        env.add_dependency(root_id, dep_b, &regular_dep("alice/b"));
        env.add_dependency(dep_a, dep_c, &regular_dep("alice/c"));

        let graph = render_graph_json(&env, root_id, &HashSet::new());
        expect![[r#"
            {
              "root": 3,
              "modules": [
                {
                  "name": "alice/a",
                  "version": "0.1.0",
                  "source": {
                    "kind": "local",
                    "path": "/workspace/a"
                  },
                  "workspace_member": false
                },
                {
                  "name": "alice/b",
                  "version": "0.1.0",
                  "source": {
                    "kind": "local",
                    "path": "/workspace/b"
                  },
                  "workspace_member": false
                },
                {
                  "name": "alice/c",
                  "version": "0.1.0",
                  "source": {
                    "kind": "local",
                    "path": "/workspace/c"
                  },
                  "workspace_member": false
                },
                {
                  "name": "alice/root",
                  "version": "0.1.0",
                  "source": {
                    "kind": "local",
                    "path": "/workspace/root"
                  },
                  "workspace_member": false
                }
              ],
              "edges": [
                {
                  "from": 0,
                  "to": 2,
                  "name": "alice/c",
                  "kind": "regular"
                },
                {
                  "from": 3,
                  "to": 0,
                  "name": "alice/a",
                  "kind": "regular"
                },
                {
                  "from": 3,
                  "to": 1,
                  "name": "alice/b",
                  "kind": "regular"
                }
              ]
            }"#]]
        .assert_eq(&serde_json::to_string_pretty(&graph).unwrap());
    }

    #[test]
    fn tree_json_includes_local_dependency_source() {
        let (roots, root_id) = ResolvedModule::only_one_module(
            local_source("username/hello", "0.1.0", "/workspace/hello"),
            local_module("username/hello", "0.1.0"),
        );
        let mut env = ResolvedEnv::from_root_modules(roots);
        let dep_id = env.add_module(
            local_source("just/hello004", "0.1.0", "/workspace/hello/deps/hello004"),
            local_module("just/hello004", "0.1.0"),
        );
        env.add_dependency(root_id, dep_id, &regular_dep("just/hello004"));

        let graph = render_graph_json(&env, root_id, &HashSet::new());
        expect![[r#"
            {
              "root": 1,
              "modules": [
                {
                  "name": "just/hello004",
                  "version": "0.1.0",
                  "source": {
                    "kind": "local",
                    "path": "/workspace/hello/deps/hello004"
                  },
                  "workspace_member": false
                },
                {
                  "name": "username/hello",
                  "version": "0.1.0",
                  "source": {
                    "kind": "local",
                    "path": "/workspace/hello"
                  },
                  "workspace_member": false
                }
              ],
              "edges": [
                {
                  "from": 1,
                  "to": 0,
                  "name": "just/hello004",
                  "kind": "regular"
                }
              ]
            }"#]]
        .assert_eq(&serde_json::to_string_pretty(&graph).unwrap());
    }

    #[test]
    fn tree_json_marks_workspace_members() {
        let mut roots = moonutil::resolution::ResolvedRootModules::with_key();
        let app = roots.insert(ResolvedModule::new(
            local_source("alice/app", "0.1.0", "/workspace/app"),
            local_module("alice/app", "0.1.0"),
        ));
        let liba = roots.insert(ResolvedModule::new(
            local_source("alice/liba", "0.1.1", "/workspace/liba"),
            local_module("alice/liba", "0.1.1"),
        ));
        let mut env = ResolvedEnv::from_root_modules(roots);
        env.add_dependency(app, liba, &regular_dep("alice/liba"));

        let workspace_members = [app, liba].into_iter().collect();
        let graph = render_graph_json(&env, app, &workspace_members);
        expect![[r#"
            {
              "root": 0,
              "modules": [
                {
                  "name": "alice/app",
                  "version": "0.1.0",
                  "source": {
                    "kind": "local",
                    "path": "/workspace/app"
                  },
                  "workspace_member": true
                },
                {
                  "name": "alice/liba",
                  "version": "0.1.1",
                  "source": {
                    "kind": "local",
                    "path": "/workspace/liba"
                  },
                  "workspace_member": true
                }
              ],
              "edges": [
                {
                  "from": 0,
                  "to": 1,
                  "name": "alice/liba",
                  "kind": "regular"
                }
              ]
            }"#]]
        .assert_eq(&serde_json::to_string_pretty(&graph).unwrap());
    }
}
