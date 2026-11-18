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

use std::{collections::BTreeSet, path::Path};

use anyhow::bail;
use moonbuild::execution::{BuildConfig, OutputStyle};
use moonbuild_rupes_recta::{
    CompileConfig, DiscoveredProject, ResolvedProject,
    build_lower::WarningCondition,
    build_plan::InputDirective,
    model::{DebugInfoRequest, DebugSymbols, PackageId, TargetKind},
};
use moonutil::{
    build_options::RunMode,
    cli_support::UniversalFlags,
    cond_expr::OptLevel as BuildProfile,
    constants::{BLACKBOX_TEST_PATCH, WHITEBOX_TEST_PATCH},
    features::FeatureGate,
    package::SupportedTargetsDeclKind,
    target::{SurfaceTarget, TargetBackend},
    test_metadata::DiagnosticLevel,
    user_log::UserLog,
};
use tracing::{Level, info, instrument};

#[derive(Debug, clap::Parser, Clone)]
pub(super) struct BuildFlags {
    /// Emit debug information
    #[clap(long, short = 'g')]
    pub debug: bool,

    /// Compile in release mode
    #[clap(long, conflicts_with = "debug")]
    pub release: bool,

    /// Enable stripping debug information
    #[clap(long, conflicts_with = "no_strip")]
    pub strip: bool,

    /// Disable stripping debug information
    #[clap(long, conflicts_with = "strip")]
    pub no_strip: bool,

    /// Select output target
    #[clap(long, value_delimiter = ',')]
    pub target: Vec<SurfaceTarget>,

    /// Enable coverage instrumentation
    #[clap(long)]
    pub enable_coverage: bool,

    /// Sort input files
    #[clap(long)]
    pub sort_input: bool,

    /// Output WAT instead of WASM
    // TODO: we need a more general name, like `--emit-asm` or even `--emit={binary,asm}`
    #[clap(long)]
    pub output_wat: bool,

    /// Treat all warnings as errors
    #[clap(long, short)]
    pub deny_warn: bool,

    /// Don't render diagnostics (in raw human-readable format)
    #[clap(long)]
    pub no_render: bool,

    /// Output diagnostics in JSON format
    #[clap(long, conflicts_with = "no_render")]
    pub output_json: bool,

    /// Warn list config
    #[clap(long, allow_hyphen_values = true)]
    pub warn_list: Option<String>,

    /// Enable value tracing
    #[clap(long, hide = true)]
    pub enable_value_tracing: bool,

    /// Set the max number of jobs to run in parallel
    #[clap(short = 'j', long)]
    pub jobs: Option<usize>,

    /// Render no-location diagnostics starting from a certain level
    #[clap(long, value_name = "MIN_LEVEL", default_value = "error")]
    pub render_no_loc: DiagnosticLevel,

    /// Limit the number of rendered diagnostics
    #[clap(long = "diagnostic-limit", value_name = "N")]
    pub diagnostic_limit: Option<usize>,
}

impl Default for BuildFlags {
    fn default() -> Self {
        Self {
            debug: false,
            release: false,
            strip: false,
            no_strip: false,
            target: Vec::new(),
            enable_coverage: false,
            sort_input: false,
            output_wat: false,
            deny_warn: false,
            no_render: false,
            output_json: false,
            warn_list: None,
            enable_value_tracing: false,
            jobs: None,
            render_no_loc: DiagnosticLevel::Error,
            diagnostic_limit: None,
        }
    }
}

impl BuildFlags {
    /// Construct the final configuration for a resolved project and its selected backend.
    ///
    /// Commands select the Target Backend before package resolution. CLI policy is
    /// resolved here while both the original flags and that backend are available;
    /// RR receives one completed configuration with no CLI parsing types.
    #[instrument(level = Level::DEBUG, skip_all)]
    pub(super) fn compile_config(
        &self,
        cli: &UniversalFlags,
        target_backend: TargetBackend,
        target_dir: &Path,
        action: RunMode,
        user_log: &UserLog,
        resolved_project: &ResolvedProject,
    ) -> anyhow::Result<CompileConfig> {
        // A couple of debug things:
        if cli.unstable_feature.rr_export_module_graph {
            info!("Exporting module graph DOT file");
            moonbuild_rupes_recta::util::print_module_graph_dot(
                &resolved_project.module_graph,
                &mut std::fs::File::create(target_dir.join("module_graph.dot"))?,
            )?;
        }
        if cli.unstable_feature.rr_export_package_graph {
            info!("Exporting package graph DOT file");
            // Import conditions make each backend's package graph distinct.
            let file_name = format!("package_graph.{}.dot", target_backend.to_dir_name());
            moonbuild_rupes_recta::util::print_package_relations_dot(
                &resolved_project.package_relations[&target_backend],
                &resolved_project.pkg_dirs,
                &mut std::fs::File::create(target_dir.join(file_name))?,
            )?;
        }

        // TODO: remove this once LLVM backend is well supported
        if target_backend == TargetBackend::LLVM {
            user_log.warn(
                "LLVM backend is experimental and only supported on nightly moonbit toolchain for now",
            );
        }
        warn_local_legacy_supported_targets(resolved_project, user_log);

        let opt_level = self.effective_profile(action);
        let strip = self.strip_for(action);
        let debug_info = DebugInfoRequest {
            symbols: if strip {
                DebugSymbols::None
            } else if action == RunMode::Run
                && target_backend == TargetBackend::Native
                && !self.debug
                && !self.no_strip
            {
                // Planning chooses how to retain source backtraces after selecting
                // generated C or direct object output for this Native run.
                DebugSymbols::Backtrace
            } else {
                DebugSymbols::Full
            },
            // Stripping symbols does not disable a debug-profile run's native
            // backtrace machinery. Other commands enable it with debug symbols.
            runtime_backtrace: target_backend.is_native()
                && (!strip || (action == RunMode::Run && opt_level == BuildProfile::Debug)),
        };
        let backend = moonbuild::compile::prepare_backend(
            target_backend,
            self.output_wat,
            cli.unstable_feature.wasi_link,
        )?;
        info!("Final backend configuration: {:?}", backend);
        let mut config = moonbuild::compile::prepare_config(
            resolved_project,
            target_dir,
            backend,
            opt_level,
            action,
            debug_info,
            cli.dry_run,
        )?;
        config.enable_coverage = self.enable_coverage;
        config.debug_export_build_plan = cli.unstable_feature.rr_export_build_plan;
        // In legacy impl, dry run always forces no JSON.
        config.moonc_output_json = !cli.dry_run && self.output_style().needs_moonc_json();
        config.warning_condition = if self.deny_warn {
            WarningCondition::Deny
        } else {
            WarningCondition::Default
        };
        config.warn_list = self.warn_list.clone();
        Ok(config)
    }

    pub(super) fn execution_config(
        &self,
        unstable_features: &FeatureGate,
        verbose: bool,
    ) -> BuildConfig {
        BuildConfig {
            parallelism: self.jobs,
            output_style: self.output_style(),
            render_no_loc: self.render_no_loc,
            diagnostic_limit: self.diagnostic_limit,
            n2_explain: unstable_features.rr_n2_explain,
            verbose,
            ..Default::default()
        }
    }

    pub fn resolve_single_target_backend(&self) -> anyhow::Result<Option<TargetBackend>> {
        if self.target.is_empty() {
            return Ok(None);
        }
        let targets = &self.target;

        if targets.len() > 1 {
            bail!("`--target` only supports one target backend");
        }
        let backends = moonutil::target::lower_surface_targets(targets);
        if backends.len() == 1 {
            Ok(Some(backends[0]))
        } else {
            bail!("`--target` only supports one target backend");
        }
    }
}

impl BuildFlags {
    /// Resolve the effective build profile for Rupes Recta compilation.
    pub fn effective_profile(&self, run_mode: RunMode) -> BuildProfile {
        if self.debug {
            BuildProfile::Debug
        } else if self.release {
            BuildProfile::Release
        } else {
            match run_mode {
                RunMode::Bench | RunMode::Bundle => BuildProfile::Release,
                RunMode::Build
                | RunMode::Run
                | RunMode::Test
                | RunMode::Check
                | RunMode::Prove
                | RunMode::Format => BuildProfile::Debug,
            }
        }
    }

    /// Resolve whether to strip debug info for Rupes Recta compilation.
    pub fn strip_for(&self, run_mode: RunMode) -> bool {
        if self.strip {
            true
        } else if self.no_strip {
            false
        } else {
            self.effective_profile(run_mode) == BuildProfile::Release
        }
    }

    pub fn output_style(&self) -> OutputStyle {
        match (self.no_render, self.output_json) {
            (true, false) => OutputStyle::Raw,
            (false, true) => OutputStyle::Json,
            (false, false) => OutputStyle::Fancy,
            (true, true) => unreachable!(
                "unreachable: both no_render and output_json flags are set (should be prevented by conflicts_with)"
            ),
        }
    }
}

fn warn_local_legacy_supported_targets(discovered: &DiscoveredProject, user_log: &UserLog) {
    let mut warned = BTreeSet::new();
    for &module_id in discovered.local_modules() {
        if let Some(pkgs) = discovered.pkg_dirs.packages_for_module(module_id) {
            for &pkg_id in pkgs.values() {
                if !warned.insert(pkg_id) {
                    continue;
                }
                let pkg = discovered.pkg_dirs.get_package(pkg_id);
                if pkg.supported_targets_decl == SupportedTargetsDeclKind::LegacyArray {
                    user_log.warn(format!(
                        "Package `{}` uses legacy array syntax for `supported_targets`; use expression syntax like `<backend>` instead",
                        pkg.fqn
                    ));
                }
            }
        }
    }
}

/// Interpret a patch filename, using the command-selected target kind when
/// it has no explicit whitebox or blackbox suffix.
pub(super) fn build_patch_directive_for_package(
    pkg: PackageId,
    value_tracing: Option<PackageId>,
    patch_file: Option<&Path>,
    default_patch_kind: TargetKind,
) -> anyhow::Result<InputDirective> {
    let patch_directive = if let Some(path) = patch_file {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("patch file path is not valid utf-8"))?;
        let kind = if path_str.ends_with(WHITEBOX_TEST_PATCH) {
            TargetKind::WhiteboxTest
        } else if path_str.ends_with(BLACKBOX_TEST_PATCH) {
            TargetKind::BlackboxTest
        } else {
            default_patch_kind
        };
        Some((pkg.build_target(kind), path.to_path_buf()))
    } else {
        None
    };

    Ok(InputDirective {
        specify_no_mi_for: None,
        specify_patch_file: patch_directive,
        value_tracing,
    })
}

#[cfg(test)]
mod tests {
    use clap::{Parser, error::ErrorKind};

    use crate::cli::MoonBuildCli;

    #[test]
    fn build_commands_reject_removed_stdlib_flags() {
        for command in [
            "build", "check", "run", "test", "bench", "bundle", "doc", "info", "prove",
        ] {
            for flag in ["--std", "--nostd"] {
                let error = MoonBuildCli::try_parse_from(["moon", command, flag])
                    .expect_err("removed standard-library flags must be rejected");
                assert_eq!(error.kind(), ErrorKind::UnknownArgument, "{command} {flag}");
            }
        }
    }
}
