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
use std::path::PathBuf;

use anyhow::Context;
use moonbuild_rupes_recta::{
    model::{PackageId, TargetKind},
    resolve::{ResolveConfig, ResolveOutput, resolve_synced_project, sync_dependencies},
};
use mooncake::pkg::tree::TreeSubcommand;
use moonutil::{
    cli_support::UniversalFlags,
    command_output::CommandOutput,
    manifest::read_module_desc_file_in_dir,
    project::PackageDirs,
    resolution::{DependencyKind, ModuleId, ModuleName, ModuleSourceKind, ResolvedEnv},
    user_log::{UserLog, UserLogCapture, UserLogEntry},
};
use serde::Serialize;

use super::require_selected_module;
use super::runtime::INTERNAL_ERROR_EXIT_CODE;

const MODULE_TREE_JSON_VERSION: u32 = 1;
const PACKAGE_TREE_JSON_VERSION: u32 = 2;

pub(crate) fn tree_cli(
    cli: UniversalFlags,
    _cmd: TreeSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let (env, root) = resolve_selected_tree(&cli, output.user_log())?;
    output.write_result(|writer| -> anyhow::Result<()> {
        writer.write_all(render_tree(&env, root).as_bytes())?;
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

fn resolve_selected_tree(
    cli: &UniversalFlags,
    user_log: &UserLog,
) -> anyhow::Result<(ResolvedEnv, ModuleId)> {
    let (module_dir, dirs) = selected_tree_project(cli, user_log)?;
    let resolved = mooncake::pkg::tree::tree(&module_dir, &dirs.project_manifest, user_log)?;
    Ok((resolved.env, resolved.root))
}

/// Resolve the package-level dependency graph through the Rupes Recta
/// pipeline, together with the module the tree is rooted at.
fn resolve_selected_package_graph(
    cli: &UniversalFlags,
    user_log: &UserLog,
) -> anyhow::Result<(ResolveOutput, ModuleId)> {
    let (module_dir, dirs) = selected_tree_project(cli, user_log)?;
    let resolve_cfg =
        ResolveConfig::new_with_load_defaults(false, false, false, cli.workspace_env.clone());
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

pub(crate) fn run_tree_json(
    cli: &UniversalFlags,
    cmd: &TreeSubcommand,
    output: &CommandOutput,
) -> TreeJsonOutcome {
    let user_log = output.user_log();
    let data = if cmd.package {
        match resolve_selected_package_graph(cli, user_log) {
            Ok((resolve_output, selected)) => {
                TreeJsonData::Package(Box::new(resolve_output), selected)
            }
            Err(error) => return TreeJsonOutcome::from_error(format!("{error:#}")).with_package(),
        }
    } else {
        match resolve_selected_tree(cli, user_log) {
            Ok((env, root)) => TreeJsonData::Module(Box::new(env), root),
            Err(error) => return TreeJsonOutcome::from_error(format!("{error:#}")),
        }
    };
    TreeJsonOutcome {
        exit_code: 0,
        accumulator: TreeJsonAccumulator {
            data: Some(data),
            error: None,
            package_mode: cmd.package,
        },
    }
}

pub(crate) struct TreeJsonOutcome {
    exit_code: i32,
    accumulator: TreeJsonAccumulator,
}

#[derive(Debug)]
enum TreeJsonData {
    Module(Box<ResolvedEnv>, ModuleId),
    Package(Box<ResolveOutput>, ModuleId),
}

#[derive(Debug, Default)]
struct TreeJsonAccumulator {
    data: Option<TreeJsonData>,
    error: Option<String>,
    package_mode: bool,
}

impl TreeJsonOutcome {
    pub(crate) fn from_error(error: impl std::fmt::Display) -> Self {
        Self {
            exit_code: INTERNAL_ERROR_EXIT_CODE,
            accumulator: TreeJsonAccumulator {
                data: None,
                error: Some(error.to_string()),
                package_mode: false,
            },
        }
    }

    fn with_package(mut self) -> Self {
        self.accumulator.package_mode = true;
        self
    }

    pub(crate) fn exit_code(&self) -> i32 {
        self.exit_code
    }
}

pub(crate) fn write_tree_json(
    output: &CommandOutput,
    capture: &UserLogCapture,
    outcome: TreeJsonOutcome,
) -> anyhow::Result<()> {
    let TreeJsonOutcome {
        exit_code,
        accumulator,
    } = outcome;
    let TreeJsonAccumulator {
        data,
        error,
        package_mode,
    } = accumulator;
    let status = if exit_code == 0 { "success" } else { "failure" };
    let logs = capture.take();
    output.write_result(|writer| -> anyhow::Result<()> {
        match data {
            Some(TreeJsonData::Module(env, root)) => {
                let graph = render_graph_json(&env, root);
                let report = TreeJsonReport {
                    version: MODULE_TREE_JSON_VERSION,
                    status,
                    error,
                    root: Some(graph.root),
                    modules: graph.modules,
                    edges: graph.edges,
                    logs,
                };
                serde_json::to_writer(&mut *writer, &report)?;
            }
            Some(TreeJsonData::Package(resolve_output, selected)) => {
                let report =
                    render_package_json_report(&resolve_output, selected, status, error, logs);
                serde_json::to_writer(&mut *writer, &report)?;
            }
            None if package_mode => {
                let report = PackageJsonReport {
                    version: PACKAGE_TREE_JSON_VERSION,
                    status,
                    error,
                    root: vec![],
                    nodes: vec![],
                    edges: vec![],
                    logs,
                };
                serde_json::to_writer(&mut *writer, &report)?;
            }
            None => {
                let report = TreeJsonReport {
                    version: MODULE_TREE_JSON_VERSION,
                    status,
                    error,
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

fn render_graph_json(resolved: &ResolvedEnv, root: ModuleId) -> TreeGraphJSON {
    let workspace_members = workspace_members(resolved);
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
    error: Option<String>,
    logs: Vec<UserLogEntry>,
) -> PackageJsonReport {
    let graph = render_package_graph_json(resolve_output, selected_module);
    PackageJsonReport {
        version: PACKAGE_TREE_JSON_VERSION,
        status,
        error,
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

fn selected_source_packages(resolve_output: &ResolveOutput, module: ModuleId) -> Vec<PackageId> {
    resolve_output
        .pkg_dirs
        .packages_for_module(module)
        .into_iter()
        .flat_map(|packages| packages.values().copied())
        .collect()
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

fn render_tree(resolved: &ResolvedEnv, root: ModuleId) -> String {
    let workspace_members = workspace_members(resolved);

    let mut out = String::new();
    out.push_str(&format_module_label(resolved, root, &workspace_members));
    out.push_str(":\n");

    let mut stack = HashSet::new();
    stack.insert(root);
    let direct_dep_count =
        render_tree_edges(resolved, root, "", &workspace_members, &mut stack, &mut out);

    if direct_dep_count == 0 {
        out.push_str("  (no dependencies)\n");
    }

    out
}

fn render_tree_edges(
    resolved: &ResolvedEnv,
    source: ModuleId,
    indent: &str,
    workspace_members: &HashSet<ModuleId>,
    stack: &mut HashSet<ModuleId>,
    out: &mut String,
) -> usize {
    let mut deps = resolved.deps_keyed(source).collect::<Vec<_>>();
    deps.sort_by(|(lhs_id, lhs_edge), (rhs_id, rhs_edge)| {
        lhs_edge.name.cmp(&rhs_edge.name).then_with(|| {
            resolved
                .module_source(*lhs_id)
                .cmp(resolved.module_source(*rhs_id))
        })
    });

    for (idx, (dep_id, dep_edge)) in deps.iter().enumerate() {
        let is_last = idx + 1 == deps.len();
        let branch = if is_last { "└─" } else { "├─" };
        out.push_str(indent);
        out.push_str(branch);
        out.push(' ');
        out.push_str(&format!(
            "{} -> {}",
            dep_edge.name,
            format_module_label(resolved, *dep_id, workspace_members)
        ));
        out.push('\n');

        let next_indent = format!("{indent}{}", if is_last { "   " } else { "│  " });
        if stack.contains(dep_id) {
            out.push_str(&next_indent);
            out.push_str("└─ (cycle)\n");
            continue;
        }

        stack.insert(*dep_id);
        render_tree_edges(
            resolved,
            *dep_id,
            &next_indent,
            workspace_members,
            stack,
            out,
        );
        stack.remove(dep_id);
    }

    deps.len()
}

fn workspace_members(resolved: &ResolvedEnv) -> HashSet<ModuleId> {
    if resolved.input_module_ids().len() > 1 {
        resolved.input_module_ids().iter().copied().collect()
    } else {
        HashSet::new()
    }
}

fn format_module_label(
    resolved: &ResolvedEnv,
    id: ModuleId,
    workspace_members: &HashSet<ModuleId>,
) -> String {
    let mut label = resolved.module_source(id).to_string();
    if workspace_members.contains(&id) {
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

        let rendered = render_tree(&env, root_id);
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

        let rendered = render_tree(&env, root_id);
        expect![[r#"
            username/hello@0.1.0 (local /workspace/hello):
            └─ just/hello004 -> just/hello004@0.1.0 (local /workspace/hello/deps/hello004)
        "#]]
        .assert_eq(&rendered);
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

        let rendered = render_tree(&env, app);
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

        let graph = render_graph_json(&env, root_id);
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

        let graph = render_graph_json(&env, root_id);
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

        let graph = render_graph_json(&env, app);
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
