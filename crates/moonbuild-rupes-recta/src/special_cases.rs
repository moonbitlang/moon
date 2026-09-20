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

//! A place **dedicated** for identifying special cases in building.
//!
//! Although the special case handlers themselves may be better living in the
//! relevant modules, we should at least keep all special case identifiers here
//! to keep them in one place for easier maintenance.
//!
//! Most, if not all, of the special cases are related to `moonbitlang/core`,
//! the standard library of MoonBit.

use crate::pkg_name::PackageFQN;

// string segments
const MOONBIT: &str = "moonbitlang";
const CORE: &str = "core";
const ABORT: &str = "abort";
const BUILTIN: &str = "builtin";
const COVERAGE: &str = "coverage";
pub(crate) const CORE_MODULE: &str = "moonbitlang/core";
pub(crate) const CORE_MODULE_TUPLE: (&str, &str) = (MOONBIT, CORE);

/// Libraries that should not be tested
const SKIP_TEST_LIBS: &[(&str, &str, &str)] = &[(MOONBIT, CORE, ABORT)];
/// Libraries that should not be covered
const SKIP_COVERAGE_LIBS: &[(&str, &str, &str)] = &[(MOONBIT, CORE, ABORT)];
/// Libraries that should use themselves for coverage
const SELF_COVERAGE_LIBS: &[(&str, &str, &str)] =
    &[(MOONBIT, CORE, BUILTIN), (MOONBIT, CORE, COVERAGE)];

pub(crate) fn module_name_is_core(name: &str) -> bool {
    name == CORE_MODULE
}

fn name_matches(package_fqn: &PackageFQN, target: (&str, &str, &str)) -> bool {
    *package_fqn == target
}

pub(crate) fn should_skip_tests(package_fqn: &PackageFQN) -> bool {
    SKIP_TEST_LIBS
        .iter()
        .any(|&target| name_matches(package_fqn, target))
}

pub(crate) fn should_skip_coverage(package_fqn: &PackageFQN) -> bool {
    SKIP_COVERAGE_LIBS
        .iter()
        .any(|&target| name_matches(package_fqn, target))
}

pub(crate) fn is_self_coverage_lib(package_fqn: &PackageFQN) -> bool {
    SELF_COVERAGE_LIBS
        .iter()
        .any(|&target| name_matches(package_fqn, target))
}
