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

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use moonutil::common::read_module_desc_file_in_dir;
use moonutil::mooncakes::result::ResolvedEnv;
use moonutil::mooncakes::{ModuleId, ModuleName};

use crate::pkg::roots_for_selected_module;
use crate::registry;
use crate::resolver::{ResolveConfig, resolve_with_default_env_and_resolver};

/// Display the dependency tree
#[derive(Debug, clap::Parser)]
pub struct TreeSubcommand {}

fn render_tree(resolved: &ResolvedEnv, root: ModuleId) -> String {
    let workspace_members = if resolved.input_module_ids().len() > 1 {
        resolved
            .input_module_ids()
            .iter()
            .copied()
            .collect::<HashSet<_>>()
    } else {
        HashSet::new()
    };

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

pub fn tree(
    project_root: &Path,
    module_dir: &Path,
    project_manifest_path: Option<&Path>,
) -> anyhow::Result<i32> {
    let module = Arc::new(read_module_desc_file_in_dir(module_dir)?);
    let roots = roots_for_selected_module(
        project_root,
        module_dir,
        Arc::clone(&module),
        project_manifest_path,
    )?;
    let resolve_cfg = ResolveConfig {
        registry: registry::default_registry(),
        inject_std: false,
    };
    let resolved = resolve_with_default_env_and_resolver(&resolve_cfg, roots)?;

    let module_name: ModuleName = module.name.as_str().into();
    let selected_root = resolved
        .input_module_ids()
        .iter()
        .copied()
        .find(|id| resolved.module_source(*id).name() == &module_name)
        .or_else(|| resolved.input_module_ids().first().copied())
        .context("resolved dependency graph has no root modules")?;

    print!("{}", render_tree(&resolved, selected_root));
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;
    use moonutil::module::MoonMod;
    use moonutil::mooncakes::result::{DependencyEdge, DependencyKind, ResolvedModule};
    use moonutil::mooncakes::{ModuleSource, ModuleSourceKind};

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
        let mut roots = moonutil::mooncakes::result::ResolvedRootModules::with_key();
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
}
