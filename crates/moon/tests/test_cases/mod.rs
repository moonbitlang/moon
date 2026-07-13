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

use std::io::Write;

use expect_test::expect_file;

use super::*;
use expect_test::expect;
use moonutil::{
    constants::{
        BUILD_DIR, MBTI_GENERATED, MOON_BIN_DIR, MOON_MOD_JSON, MOON_NO_WORKSPACE, MOON_WORK_ENV,
    },
    manifest::MoonModJSON,
    path::CargoPathExt,
    target::TargetBackend,
    text::StringExt,
    version::get_cargo_pkg_version,
};
use walkdir::WalkDir;

mod abort_override;
mod backend;
mod backend_config;
mod bench2;
mod blackbox;
mod build_package_dep_core;
mod build_workflow;
mod check_fmt;
mod check_watch;
mod circle_pkg_ab_001_test;
mod clean;
mod cond_comp;
mod debug_flag_test;
mod dedup_diag;
mod dep_order;
mod design;
mod diagnostics_format;
mod diamond_pkg;
mod docs_examples;
mod dummy_core;
mod extra_flags;
mod fancy_import;
mod filter_by_path;
mod fmt;
mod fmt_ignore;
mod fmt_moon_mod;
mod fmt_moon_pkg;
mod fmt_path;
mod fuzzy_matching;
mod hello;
mod indirect_dep;
mod inline_test;
#[cfg(target_os = "macos")]
mod install_atomic_rename;
mod js_test_build_only;
mod main_package;
#[cfg(unix)]
mod mbti;
mod moon_bench;
mod moon_build_package;
mod moon_bundle;
mod moon_commands;
mod moon_coverage;
mod moon_info_001;
mod moon_info_002;
mod moon_info_compare_backends;
mod moon_install_global;
mod moon_new;
mod moon_prove;
mod moon_test;
mod moon_version;
#[cfg(unix)]
mod native_abort_trace;
mod native_backend;
mod native_stub_stability;
mod no_export_when_test;
mod output_format;
mod package_management;
mod package_metadata;
mod package_testing;
mod packages;
mod prebuild;
mod prebuild_config_script;
mod prebuild_link_config_self;
mod query_symbol;
mod run_command;
mod run_doc_test;
mod run_md_test;
mod run_profile;
mod simple_pkg;
mod single_file;
mod single_file_front_matter;
mod snapshot_testing;
mod source_directory;
mod specify_source_dir_001;
mod specify_source_dir_002;
mod symlink_file_discovery;
mod target_backend;
mod targets;
mod test_dot_source;
mod test_driver_dependencies;
mod test_driver_map_collision;
mod test_error_report;
mod test_exclude_001;
mod test_exclude_002;
mod test_expect_test;
mod test_expect_with_escape;
mod test_filter;
mod test_include_001;
mod test_include_002;
mod test_include_003;
mod test_moon_info;
mod test_moonbitlang_x;
mod test_outline;
mod test_release;
mod third_party;
mod tool_commands;
mod value_tracing;
mod virtual_pkg;
mod virtual_pkg2;
mod virtual_pkg_dep;
mod virtual_pkg_test;
mod warns;
mod wasm_memory;
mod wbtest_coverage;
mod whitespace_test;
mod workspace_basic;
