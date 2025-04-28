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

pub mod cmd_builder;
pub mod gen_build;
pub mod gen_bundle;
pub mod gen_check;
pub mod gen_runtest;
pub mod n2_errors;
pub mod util;

// WORKAROUND for do not test coverage on coverage library itself
const MOON_CORE_COVERAGE_LIB: &str = "moonbitlang/core/coverage";
const MOON_CORE_BUILTIN_LIB: &str = "moonbitlang/core/builtin";
const MOON_CORE_PANIC_LIB: &str = "moonbitlang/core/panic";
const MOON_CORE_INTRINSICS_LIB: &str = "moonbitlang/core/intrinsics";

#[test]
fn test_start_with() {
    use moonutil::common::MOONBITLANG_CORE;
    assert!(MOON_CORE_COVERAGE_LIB.starts_with(MOONBITLANG_CORE));
    assert!(MOON_CORE_BUILTIN_LIB.starts_with(MOONBITLANG_CORE));
}

static SKIP_COVERAGE_LIBS: &[&str] = &[MOON_CORE_PANIC_LIB, MOON_CORE_INTRINSICS_LIB];
static SELF_COVERAGE_LIBS: &[&str] = &[MOON_CORE_BUILTIN_LIB, MOON_CORE_COVERAGE_LIB];

fn is_skip_coverage_lib(name: &str) -> bool {
    SKIP_COVERAGE_LIBS.contains(&name)
}

fn is_self_coverage_lib(name: &str) -> bool {
    SELF_COVERAGE_LIBS.contains(&name)
}

fn coverage_args(
    enable_coverage: bool,
    package_name: &str,
    package_original_name: Option<&str>,
    two_dashes: bool,
) -> Vec<String> {
    if !enable_coverage {
        return vec![];
    }
    if is_skip_coverage_lib(package_name) {
        return vec![];
    }
    let dashes = if two_dashes { "--" } else { "-" };
    let mut args = vec![format!("{}enable-coverage", dashes)];
    // WORKAROUND: lang core/builtin and core/coverage should be able to cover themselves
    if is_self_coverage_lib(package_name) {
        args.push(format!("{}coverage-package-override=@self", dashes));
    } else if let Some(original_name) = package_original_name {
        if is_self_coverage_lib(original_name) {
            args.push(format!(
                "{}coverage-package-override={}",
                dashes, original_name
            ));
        }
    }
    args
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiAlias {
    pub name: String,
    pub alias: String,
}
