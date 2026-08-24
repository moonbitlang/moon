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

use std::collections::{HashMap, HashSet};

use mooncake::pkg::tree::TreeSubcommand;
use moonutil::{
    cli_support::UniversalFlags,
    command_output::CommandOutput,
    project::PackageDirs,
    resolution::{DependencyKind, ModuleId, ModuleSourceKind, ResolvedEnv},
    user_log::{UserLog, UserLogCapture, UserLogEntry},
};
use serde::Serialize;

use super::require_selected_module;
use super::runtime::INTERNAL_ERROR_EXIT_CODE;

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

fn resolve_selected_tree(
    cli: &UniversalFlags,
    user_log: &UserLog,
) -> anyhow::Result<(ResolvedEnv, ModuleId)> {
    let project = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .select(user_log)?;
    let module_dir = require_selected_module(project.context(), "tree")?;
    let PackageDirs {
        project_manifest, ..
    } = project.package_dirs()?;
    let resolved = mooncake::pkg::tree::tree(&module_dir, &project_manifest, user_log)?;
    Ok((resolved.env, resolved.root))
}

pub(crate) fn run_tree_json(
    cli: &UniversalFlags,
    _cmd: &TreeSubcommand,
    output: &CommandOutput,
) -> TreeJsonOutcome {
    match resolve_selected_tree(cli, output.user_log()) {
        Ok((env, root)) => TreeJsonOutcome {
            exit_code: 0,
            accumulator: TreeJsonAccumulator {
                resolved: Some((env, root)),
                error: None,
            },
        },
        Err(error) => TreeJsonOutcome::from_error(format!("{error:#}")),
    }
}

pub(crate) struct TreeJsonOutcome {
    exit_code: i32,
    accumulator: TreeJsonAccumulator,
}

#[derive(Debug, Default)]
struct TreeJsonAccumulator {
    resolved: Option<(ResolvedEnv, ModuleId)>,
    error: Option<String>,
}

impl TreeJsonOutcome {
    pub(crate) fn from_error(error: impl std::fmt::Display) -> Self {
        Self {
            exit_code: INTERNAL_ERROR_EXIT_CODE,
            accumulator: TreeJsonAccumulator {
                resolved: None,
                error: Some(error.to_string()),
            },
        }
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
    let (root, modules, edges) = match accumulator.resolved {
        Some((env, root)) => {
            let graph = render_graph_json(&env, root);
            (Some(graph.root), graph.modules, graph.edges)
        }
        None => (None, vec![], vec![]),
    };
    let report = TreeJsonReport {
        version: 1,
        status: if exit_code == 0 { "success" } else { "failure" },
        error: accumulator.error,
        root,
        modules,
        edges,
        logs: capture.take(),
    };
    output.write_result(|writer| -> anyhow::Result<()> {
        serde_json::to_writer(&mut *writer, &report)?;
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
