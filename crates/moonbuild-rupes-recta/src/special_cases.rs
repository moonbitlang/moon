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

//! A place **dedicated** for identifying special cases in building.
//!
//! Although the special case handlers themselves may be better living in the
//! relevant modules, we should at least keep all special case identifiers here
//! to keep them in one place for easier maintenance.
//!
//! Most, if not all, of the special cases are related to `moonbitlang/core`,
//! the standard library of MoonBit.
#![allow(unused)]

use moonutil::package::MoonPkg;

use crate::{ResolveOutput, model::PackageId};

// string segments
const MOONBIT: &str = "moonbitlang";
const CORE: &str = "core";
const ABORT: &str = "abort";
const BUILTIN: &str = "builtin";
const COVERAGE: &str = "coverage";
const PRELUDE: &str = "prelude";
pub const CORE_MODULE: &str = "moonbitlang/core";
pub const CORE_MODULE_TUPLE: (&str, &str) = (MOONBIT, CORE);

/// Libraries that should not be tested
const SKIP_TEST_LIBS: &[(&str, &str, &str)] = &[(MOONBIT, CORE, ABORT)];
/// Libraries that should not be covered
const SKIP_COVERAGE_LIBS: &[(&str, &str, &str)] = &[(MOONBIT, CORE, ABORT)];
/// Libraries that should use themselves for coverage
const SELF_COVERAGE_LIBS: &[(&str, &str, &str)] =
    &[(MOONBIT, CORE, BUILTIN), (MOONBIT, CORE, COVERAGE)];

pub fn module_name_is_core(name: &str) -> bool {
    name == CORE_MODULE
}

/// Core packages require importing `prelude` in test imports, or the test will
/// not be able to run.
pub fn add_prelude_as_import_for_core(mut pkg_json: MoonPkg) -> MoonPkg {
    pkg_json
        .test_imports
        .push(moonutil::package::Import::Alias {
            path: "moonbitlang/core/prelude".into(),
            alias: Some("prelude".into()),
            sub_package: false,
        });
    pkg_json
}

fn name_matches(
    package_id: PackageId,
    resolve_output: &ResolveOutput,
    target: (&str, &str, &str),
) -> bool {
    let pkg = resolve_output.pkg_dirs.get_package(package_id);
    let fqn = &pkg.fqn;
    *fqn == target
}

pub fn should_skip_tests(package_id: PackageId, resolve_output: &ResolveOutput) -> bool {
    SKIP_TEST_LIBS
        .iter()
        .any(|&target| name_matches(package_id, resolve_output, target))
}

pub fn should_skip_coverage(package_id: PackageId, resolve_output: &ResolveOutput) -> bool {
    SKIP_COVERAGE_LIBS
        .iter()
        .any(|&target| name_matches(package_id, resolve_output, target))
}

pub fn is_self_coverage_lib(package_id: PackageId, resolve_output: &ResolveOutput) -> bool {
    SELF_COVERAGE_LIBS
        .iter()
        .any(|&target| name_matches(package_id, resolve_output, target))
}

pub fn is_builtin_lib(package_id: PackageId, resolve_output: &ResolveOutput) -> bool {
    name_matches(package_id, resolve_output, (MOONBIT, CORE, BUILTIN))
}
