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

//! Resolve build selectors and backend preferences into complete build requests.
//! Package identities and default linkability are decided here, before import
//! resolution; the build workflow consumes the requests without reading selectors.

use moonbuild::compile::BuildRequest;
use moonbuild_rupes_recta::{DiscoveredProject, intent::UserIntent, model::PackageId};
use moonutil::{target::TargetBackend, user_log::UserLog};

use crate::cli::selection::{
    TargetPackageGroup, ensure_packages_support_backend, group_packages_by_preferred_backend,
    local_modules_preferred_target, local_packages, match_packages_by_name_rr,
    package_supports_backend, select_packages, select_supported_packages,
};

use super::BuildSubcommand;

#[tracing::instrument(level = "debug", skip_all)]
pub(super) fn select_requests(
    discovered: &DiscoveredProject,
    cmd: &BuildSubcommand,
    selected_target_backends: &[TargetBackend],
    user_log: &UserLog,
) -> anyhow::Result<Vec<(TargetBackend, BuildRequest)>> {
    let explicit_selector = !cmd.path.is_empty() || cmd.package.is_some();
    let selections = if selected_target_backends.is_empty() {
        let packages = select_build_packages(discovered, cmd, None, user_log)?;
        let mut groups = group_packages_by_preferred_backend(discovered, packages);
        for group in &mut groups {
            group.packages.retain(|&package| {
                package_supports_backend(discovered, package, group.target_backend)
            });
        }
        groups.retain(|group| !group.packages.is_empty());

        if groups.is_empty() && !explicit_selector {
            let target_backend =
                local_modules_preferred_target(discovered, user_log).unwrap_or_default();
            // An empty preferred-backend grouping is not an empty default build:
            // packages may support the fallback even when they reject their own
            // module's preference. Keep a plan even if the fallback is also empty.
            groups.push(TargetPackageGroup {
                target_backend,
                packages: select_build_packages(discovered, cmd, Some(target_backend), user_log)?,
            });
        }
        groups
    } else {
        let mut groups = Vec::new();
        for &target_backend in selected_target_backends {
            // Explicit target order also determines selector diagnostic order.
            let packages = select_build_packages(discovered, cmd, Some(target_backend), user_log)?;
            if !explicit_selector || !packages.is_empty() {
                groups.push(TargetPackageGroup {
                    target_backend,
                    packages,
                });
            }
        }
        groups
    };

    Ok(selections
        .into_iter()
        .map(|group| {
            let packages = if explicit_selector {
                group.packages
            } else {
                default_build_packages(discovered, group.packages, group.target_backend)
            };
            let request = packages
                .into_iter()
                .map(UserIntent::Build)
                .collect::<Vec<_>>()
                .into();
            (group.target_backend, request)
        })
        .collect())
}

fn select_build_packages(
    discovered: &DiscoveredProject,
    cmd: &BuildSubcommand,
    target_backend: Option<TargetBackend>,
    user_log: &UserLog,
) -> anyhow::Result<Vec<PackageId>> {
    if !cmd.path.is_empty() {
        if let Some(target_backend) = target_backend {
            return select_supported_packages(discovered, &cmd.path, target_backend, user_log);
        }
        return Ok(select_packages(discovered, &cmd.path, user_log)?
            .into_iter()
            .map(|(_, pkg_id)| pkg_id)
            .collect());
    }

    if let Some(package_filter) = cmd.package.as_deref() {
        let packages = match_packages_by_name_rr(
            discovered,
            discovered.local_modules(),
            package_filter,
            user_log,
        );
        if let Some(target_backend) = target_backend {
            ensure_packages_support_backend(discovered, packages.iter().copied(), target_backend)?;
        }
        return Ok(packages);
    }

    Ok(local_packages(discovered)
        .filter(|&package| {
            target_backend
                .is_none_or(|backend| package_supports_backend(discovered, package, backend))
        })
        .collect())
}

/// Default builds request linkable packages, or all non-stdlib packages if none
/// are linkable. Explicit selectors never apply this default-scope rule.
fn default_build_packages(
    discovered: &DiscoveredProject,
    packages: Vec<PackageId>,
    target_backend: TargetBackend,
) -> Vec<PackageId> {
    let linkable = packages
        .iter()
        .copied()
        .filter(|&package| {
            let pkg = discovered.pkg_dirs.get_package(package);
            pkg.raw.force_link
                || pkg
                    .raw
                    .link
                    .as_ref()
                    .is_some_and(|link| link.need_link(target_backend))
                || pkg.raw.is_main
        })
        .collect::<Vec<_>>();
    if linkable.is_empty() {
        packages
            .into_iter()
            .filter(|&package| !discovered.pkg_dirs.get_package(package).is_stdlib)
            .collect()
    } else {
        linkable
    }
}
