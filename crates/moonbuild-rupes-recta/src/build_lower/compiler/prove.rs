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

use std::borrow::Cow;
use std::path::Path;

use crate::build_lower::compiler::{BuildCommonConfig, BuildCommonInput, CmdlineAbstraction};

/// Abstraction for `moonc prove`.
#[derive(Debug)]
pub(crate) struct MooncProve<'a> {
    pub required: BuildCommonInput<'a>,
    pub defaults: BuildCommonConfig<'a>,
    pub why3_config: Cow<'a, Path>,
    pub whyml_out: Cow<'a, Path>,
    pub proof_report_out: Cow<'a, Path>,
    pub single_file: bool,
    pub extra_flags: &'a [String],
}

impl<'a> CmdlineAbstraction for MooncProve<'a> {
    fn to_args(&self, args: &mut Vec<String>) {
        args.push("prove".into());

        self.defaults.add_patch_file_moonc(args);
        self.defaults.add_error_format(args);
        self.defaults.add_deny_all(args);
        self.required.add_mbt_sources(args);
        self.required.add_doctest_only_sources(args);
        self.required.add_include_doctests_if_blackbox(args);
        self.defaults.add_custom_warn_alert_lists(args);
        self.defaults.add_warn_alert_allow_all(args);
        args.extend([
            "--whyml-output-path".to_string(),
            self.whyml_out.display().to_string(),
            "--proof-report-output-path".to_string(),
            self.proof_report_out.display().to_string(),
            "--why3-config".to_string(),
            self.why3_config.display().to_string(),
        ]);
        self.required.add_package_config(args);
        self.defaults.add_is_main(args);
        if self.single_file {
            args.push("-single-file".to_string());
        }
        self.defaults.add_stdlib_path(args);
        self.required.add_mi_dependencies(args);
        self.required.add_package_sources(args);
        self.required.add_test_kind_flags(args);
        self.defaults.add_virtual_package_check(args);
        self.defaults.add_virtual_package_implementation_check(args);
        self.defaults.add_workspace_root(args);
        self.required.add_all_pkgs_json(args);

        for flag in self.extra_flags {
            args.push(flag.to_string());
        }
    }
}
