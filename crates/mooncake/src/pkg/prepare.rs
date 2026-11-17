// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Prepare project roots and the installed standard library before resolution.
//!
//! TODO: Move this project policy with build preparation when command orchestration
//! owns the selected roots and standard library. The resolver already accepts the
//! resulting graph without locating the toolchain or reading its core manifest.

use std::sync::Arc;

use anyhow::Context;
use moonutil::{
    constants::MOONBITLANG_CORE,
    manifest::read_module_desc_file_in_dir,
    resolution::{ModuleDependencyGraph, ModuleSource, ResolvedRootModules},
    toolchain,
};

use crate::resolver::prepare_roots;

/// Prepare build roots, retaining their IDs and their role as input modules.
/// Building core itself does not add the installed standard library.
pub(super) fn prepare_build_modules(
    roots: ResolvedRootModules,
) -> anyhow::Result<ModuleDependencyGraph> {
    let includes_core = roots
        .iter()
        .any(|(_, module)| module.module_info().name == MOONBITLANG_CORE);
    if includes_core && roots.len() != 1 {
        anyhow::bail!("workspaces that include `moonbitlang/core` are not supported yet");
    }

    // Reject invalid root declarations before trying to read the toolchain.
    let mut module_graph = prepare_roots(roots)?;
    if !includes_core {
        inject_std(&mut module_graph).map_err(|error| {
            anyhow::anyhow!("Cannot inject the standard library `moonbitlang/core`: {error}")
        })?;
    }
    Ok(module_graph)
}

/// Inject the definition of `moonbitlang/core` in the installation directory
/// to the resolve graph, and mark it as the standard library.
fn inject_std(module_graph: &mut ModuleDependencyGraph) -> anyhow::Result<()> {
    let core_dir = toolchain::core();
    let loaded_core = read_module_desc_file_in_dir(&core_dir).with_context(|| {
        if core_dir.exists() {
            format!(
                "Cannot load the core file. The standard library at `{}` \
                 does not appear to be bundled. Run `moon -C {} bundle --target all` \
                 to bundle it",
                core_dir.display(),
                core_dir.display()
            )
        } else {
            format!(
                "Cannot load the core file. The standard library directory `{}` \
                 does not exist. Reinstall the MoonBit toolchain, or set \
                 $MOON_CORE_OVERRIDE to a checkout of moonbitlang/core",
                core_dir.display()
            )
        }
    })?;
    let source = ModuleSource::from_stdlib(&loaded_core, &core_dir)?;
    let id = module_graph.add_module(source, Arc::new(loaded_core));
    module_graph.register_stdlib(id);

    Ok(())
}
