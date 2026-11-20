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

//! Self coverage for builtin and the coverage runtime itself.
//!
//! The coverage runtime depends on builtin, so instrumenting builtin cannot add
//! an ordinary dependency back to coverage. When developing core with coverage
//! enabled, [`augment_builtin_sources`] adds the runtime's source paths to
//! builtin after discovery. It changes only the in-memory file set; the coverage
//! package keeps its sources and no files are copied or rewritten.
//!
//! Both packages then use their own runtime definitions: package resolution
//! excludes them from implicit coverage imports, and lowering selects `@self`
//! for compiler instrumentation and test-driver collection. Those stages share
//! [`uses_self_runtime`] while retaining their own graph and command generation.
//!
//! TODO: Remove source augmentation when the compiler/core contract provides
//! builtin's coverage runtime without borrowing another package's sources.

use moonutil::resolution::ModuleDependencyGraph;
use tracing::{instrument, warn};

use super::CORE_MODULE_TUPLE;
use crate::{
    discover::DiscoverResult,
    pkg_name::{PackageFQN, PackagePath},
};

const BUILTIN: &str = "builtin";
const COVERAGE: &str = "coverage";
const SELF_COVERAGE_LIBS: &[(&str, &str, &str)] = &[
    (CORE_MODULE_TUPLE.0, CORE_MODULE_TUPLE.1, BUILTIN),
    (CORE_MODULE_TUPLE.0, CORE_MODULE_TUPLE.1, COVERAGE),
];

/// Whether instrumentation and collection use the package's own runtime.
pub(crate) fn uses_self_runtime(package_fqn: &PackageFQN) -> bool {
    SELF_COVERAGE_LIBS.iter().any(|&name| *package_fqn == name)
}

/// Add coverage sources to builtin when core is the sole project root.
///
/// Call only when coverage is enabled, after scanning and before resolving
/// packages. Keeping this timing preserves the merged file set seen by target
/// selection and metadata as well as compilation.
#[instrument(skip_all)]
pub(crate) fn augment_builtin_sources(modules: &ModuleDependencyGraph, res: &mut DiscoverResult) {
    let &[core] = modules.input_module_ids() else {
        return;
    };
    if *modules.module_source(core).name() != CORE_MODULE_TUPLE {
        return;
    }

    let Some(map) = res.packages_for_module(core) else {
        return;
    };

    let builtin_path = PackagePath::new(BUILTIN).expect("builtin is a valid package path");
    let coverage_path = PackagePath::new(COVERAGE).expect("coverage is a valid package path");

    let Some(&builtin_id) = map.get(&builtin_path) else {
        warn!("No builtin package found in core module, skipping coverage->builtin injection");
        return;
    };
    let Some(&coverage_id) = map.get(&coverage_path) else {
        warn!("No coverage package found in core module, skipping coverage->builtin injection");
        return;
    };

    let coverage_files = res.get_package(coverage_id).source_files.clone();
    let builtin = res.get_package_mut(builtin_id);

    builtin.source_files.extend(coverage_files);
    builtin.source_files.sort();
}
