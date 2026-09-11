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

use std::borrow::Cow;
use std::path::Path;

use crate::build_lower::compiler::{
    BuildCommonConfig, BuildCommonInput, CmdlineAbstraction, DepProof,
};

/// Abstraction for `moonc prove`.
#[derive(Debug)]
pub(crate) struct MooncProve<'a> {
    pub required: BuildCommonInput<'a>,
    pub defaults: BuildCommonConfig<'a>,
    pub whyml_out: Cow<'a, Path>,
    pub proof_report_out: Option<Cow<'a, Path>>,
    pub why3_config: Option<Cow<'a, Path>>,
    pub why3_loadpaths: Vec<std::path::PathBuf>,
    pub dep_proofs: Vec<DepProof<'a>>,
    pub emit_only: bool,
    pub single_file: bool,
    pub extra_flags: &'a [String],
}

impl<'a> CmdlineAbstraction for MooncProve<'a> {
    fn to_args(&self, args: &mut Vec<String>) {
        args.push("prove".into());

        self.defaults.add_patch_file_moonc(args);
        self.defaults.add_error_format(args);
        self.required.add_mbt_sources(args);
        self.required.add_doctest_only_sources(args);
        self.required.add_include_doctests_if_blackbox(args);
        self.defaults.add_warning_options(args);
        args.extend([
            "-whyml-output-path".to_string(),
            self.whyml_out.display().to_string(),
        ]);
        if let Some(proof_report_out) = &self.proof_report_out {
            args.extend([
                "-proof-report-output-path".to_string(),
                proof_report_out.display().to_string(),
            ]);
        }
        if let Some(why3_config) = &self.why3_config {
            args.extend([
                "-why3-config".to_string(),
                why3_config.display().to_string(),
            ]);
        }
        for loadpath in &self.why3_loadpaths {
            args.extend(["-why3-loadpath".to_string(), loadpath.display().to_string()]);
        }
        for dep_proof in &self.dep_proofs {
            args.extend(["-dep-proof".to_string(), dep_proof.to_arg()]);
        }
        if self.emit_only {
            args.push("-emit-only".to_string());
        }
        self.required.add_package_config(args);
        self.defaults.add_pkgtype(args);
        if self.single_file {
            args.push("-single-file".to_string());
        }
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
