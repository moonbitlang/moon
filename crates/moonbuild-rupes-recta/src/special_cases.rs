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
//! Although the special case handlers themselves are better living in the
//! relevant modules, we should keep alls special case identifiers here to
//! keep them in one place for easier maintenance.
#![allow(unused)]

use crate::{model::PackageId, ResolveOutput};

// string segments
const MOONBIT: &str = "moonbitlang";
const CORE: &str = "core";
const ABORT: &str = "abort";
const BUILTIN: &str = "builtin";
const COVERAGE: &str = "coverage";

/// Libraries that should not be tested
const SKIP_TEST_LIBS: &[(&str, &str, &str)] = &[(MOONBIT, CORE, ABORT)];
/// Libraries that should not be covered
const SKIP_COVERAGE_LIBS: &[(&str, &str, &str)] = &[(MOONBIT, CORE, ABORT)];
/// Libraries that should use themselves for coverage
const SELF_COVERAGE_LIBS: &[(&str, &str, &str)] =
    &[(MOONBIT, CORE, BUILTIN), (MOONBIT, CORE, COVERAGE)];

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
