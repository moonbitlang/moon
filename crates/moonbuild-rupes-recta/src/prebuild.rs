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

//! Module-level prebuild configuration consumed by build planning.
//!
//! The command layer owns script compilation, execution, and output validation.

use std::collections::HashMap;

use moonutil::{
    build_script::{LinkConfig, RerunIfKind},
    compiler_flags::NativeLinkArg,
    resolution::ModuleId,
};

use crate::model::PackageId;

/// The output of running prebuild config scripts
#[derive(Debug, Default)]
pub struct PrebuildOutput {
    pub module_outputs: HashMap<ModuleId, ModulePrebuildOutput>,
    pub package_configs: HashMap<PackageId, LinkConfig>,
}

impl PrebuildOutput {
    /// Append package link configuration in the caller's dependency order.
    pub(crate) fn append_link_args(
        &self,
        pkgs: impl Iterator<Item = PackageId>,
        out: &mut Vec<NativeLinkArg>,
    ) {
        for pkg in pkgs {
            let Some(link_config) = self.package_configs.get(&pkg) else {
                continue;
            };

            let link_flags = link_config
                .link_flags
                .as_ref()
                .and_then(|x| shlex::split(x));
            if let Some(link_flags) = link_flags {
                out.extend(link_flags.into_iter().map(NativeLinkArg::Opaque));
            }

            out.extend(
                link_config
                    .link_libs
                    .iter()
                    .cloned()
                    .map(NativeLinkArg::Library),
            );
            out.extend(
                link_config
                    .link_search_paths
                    .iter()
                    .map(|path| NativeLinkArg::SearchPath(path.into())),
            );
        }
    }
}

/// A module's prebuild output
#[derive(Debug)]
pub struct ModulePrebuildOutput {
    /// Conditions that might trigger rerun. Currently have no effect (always rerun).
    pub rerun_if: Vec<RerunIfKind>,
    /// Environment variables set by the prebuild script
    pub vars: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use slotmap::SlotMap;

    use super::*;

    #[test]
    fn prebuild_link_args_preserve_package_order_and_structure() {
        let mut packages = SlotMap::<PackageId, ()>::with_key();
        let first = packages.insert(());
        let second = packages.insert(());
        let unconfigured = packages.insert(());
        let prebuild = PrebuildOutput {
            package_configs: HashMap::from([
                (
                    first,
                    LinkConfig {
                        package: "test/first".to_string(),
                        link_flags: Some(r#""custom dir/custom.lib" --start-group"#.to_string()),
                        link_libs: vec!["SDL3".to_string()],
                        link_search_paths: vec!["SDL libs".to_string()],
                    },
                ),
                (
                    second,
                    LinkConfig {
                        package: "test/second".to_string(),
                        link_flags: Some("--end-group".to_string()),
                        link_libs: vec!["SDL3".to_string()],
                        link_search_paths: vec!["other libs".to_string()],
                    },
                ),
            ]),
            ..Default::default()
        };
        let mut args = vec![NativeLinkArg::Opaque("existing.lib".into())];
        prebuild.append_link_args([first, unconfigured, second].into_iter(), &mut args);

        // Do not regroup by argument kind or deduplicate: both can change
        // positional linker behavior across package boundaries.
        assert_eq!(
            args,
            [
                NativeLinkArg::Opaque("existing.lib".into()),
                NativeLinkArg::Opaque("custom dir/custom.lib".into()),
                NativeLinkArg::Opaque("--start-group".into()),
                NativeLinkArg::Library("SDL3".into()),
                NativeLinkArg::SearchPath("SDL libs".into()),
                NativeLinkArg::Opaque("--end-group".into()),
                NativeLinkArg::Library("SDL3".into()),
                NativeLinkArg::SearchPath("other libs".into()),
            ]
        );
    }
}
