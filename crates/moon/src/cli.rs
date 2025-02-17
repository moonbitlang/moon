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

pub mod build;
pub mod build_matrix;
pub mod bundle;
pub mod check;
pub mod clean;
pub mod coverage;
pub mod deps;
pub mod doc;
pub mod fmt;
pub mod generate_test_driver;
pub mod info;
pub mod mooncake_adapter;
pub mod new;
mod pre_build;
pub mod query;
pub mod run;
pub mod shell_completion;
pub mod test;
pub mod tool;
pub mod update;
pub mod upgrade;
pub mod version;

pub use build::*;
pub use build_matrix::*;
pub use bundle::*;
pub use check::*;
pub use clean::*;
pub use coverage::*;
pub use deps::*;
pub use doc::*;
pub use fmt::*;
pub use generate_test_driver::*;
pub use info::*;
use moonbuild::upgrade::UpgradeSubcommand;
use mooncake::pkg::{
    add::AddSubcommand, install::InstallSubcommand, remove::RemoveSubcommand, tree::TreeSubcommand,
};
pub use new::*;
pub use query::*;
pub use run::*;
pub use shell_completion::*;
pub use test::*;
pub use tool::*;
pub use update::*;
pub use upgrade::*;
pub use version::*;

use anyhow::bail;
use moonutil::{
    cli::UniversalFlags,
    common::{
        read_module_desc_file_in_dir, BuildPackageFlags, LinkCoreFlags, MooncOpt, OutputFormat,
        SurfaceTarget, TargetBackend, MOONBITLANG_CORE, MOON_MOD_JSON,
    },
    mooncakes::{LoginSubcommand, PackageSubcommand, PublishSubcommand, RegisterSubcommand},
};
use std::path::Path;

#[derive(Debug, clap::Parser)]
#[clap(
    name = "moon",
    about = "The build system and package manager for MoonBit."
)]
pub struct MoonBuildCli {
    #[clap(subcommand)]
    pub subcommand: MoonBuildSubcommands,

    #[clap(flatten)]
    pub flags: UniversalFlags,
}

#[derive(Debug, clap::Parser)]
pub enum MoonBuildSubcommands {
    New(NewSubcommand),

    // Build system
    Bundle(BundleSubcommand),
    Build(BuildSubcommand),
    Check(CheckSubcommand),
    Run(RunSubcommand),
    Test(TestSubcommand),
    #[clap(hide = true)]
    GenerateTestDriver(GenerateTestDriverSubcommand),
    Clean(CleanSubcommand),
    Fmt(FmtSubcommand),
    Doc(DocSubcommand),
    Info(InfoSubcommand),

    // Dependencies
    Add(AddSubcommand),
    Remove(RemoveSubcommand),
    Install(InstallSubcommand),
    Tree(TreeSubcommand),

    // Mooncake
    Login(LoginSubcommand),
    Register(RegisterSubcommand),
    Publish(PublishSubcommand),
    Package(PackageSubcommand),

    Update(UpdateSubcommand),

    // Misc
    Coverage(CoverageSubcommand),
    GenerateBuildMatrix(GenerateBuildMatrix),
    #[clap(hide = true)]
    Query(QuerySubcommand),

    /// Upgrade toolchains
    Upgrade(UpgradeSubcommand),
    ShellCompletion(ShellCompSubCommand),
    Version(VersionSubcommand),
    #[clap(hide = true)]
    Tool(ToolSubcommand),
}

#[derive(Debug, clap::Parser, Clone)]
pub struct BuildFlags {
    /// Enable the standard library (default)
    #[clap(long)]
    std: bool,

    /// Disable the standard library
    #[clap(long, long = "nostd")]
    no_std: bool,

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
    pub target: Option<Vec<SurfaceTarget>>,

    #[clap(skip)]
    pub target_backend: Option<TargetBackend>,

    /// Handle the selected targets sequentially
    #[clap(long, requires = "target")]
    pub serial: bool,

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

    /// Don't render diagnostics from moonc (don't pass '-error-format json' to moonc)
    #[clap(long)]
    pub no_render: bool,

    /// Warn list config
    #[clap(long, allow_hyphen_values = true)]
    pub warn_list: Option<String>,

    /// Alert list config
    #[clap(long, allow_hyphen_values = true)]
    pub alert_list: Option<String>,

    /// Enable value tracing
    #[clap(long, hide = true)]
    pub enable_value_tracing: bool,

    /// Set the max number of jobs to run in parallel
    #[clap(short = 'j', long)]
    pub jobs: Option<usize>,
}

impl BuildFlags {
    pub fn std(&self) -> bool {
        match (self.std, self.no_std) {
            (false, false) => true,
            (true, false) => true,
            (false, true) => false,
            (true, true) => panic!("both std and no_std flags are set"),
        }
    }

    pub fn strip(&self) -> bool {
        if self.strip {
            true
        } else if self.no_strip {
            false
        } else {
            !self.debug
        }
    }
}

pub fn get_compiler_flags(src_dir: &Path, build_flags: &BuildFlags) -> anyhow::Result<MooncOpt> {
    // read moon.mod.json
    if !moonutil::common::check_moon_mod_exists(src_dir) {
        bail!("could not find `{}`", MOON_MOD_JSON);
    }
    let moon_mod = read_module_desc_file_in_dir(src_dir)?;
    let extra_build_opt = moon_mod.compile_flags.unwrap_or_default();
    let extra_link_opt = moon_mod.link_flags.unwrap_or_default();

    let output_format = if build_flags.output_wat {
        OutputFormat::Wat
    } else {
        OutputFormat::Wasm
    };

    let target_backend = build_flags.target_backend.unwrap_or_default();

    if target_backend == TargetBackend::Js && output_format == OutputFormat::Wat {
        bail!("--output-wat is not supported for --target js");
    }

    let output_format = match target_backend {
        TargetBackend::Js => OutputFormat::Js,
        TargetBackend::Native => OutputFormat::Native,
        _ => output_format,
    };

    let debug_flag = build_flags.debug;
    let enable_coverage = build_flags.enable_coverage;
    let source_map =
        debug_flag && matches!(target_backend, TargetBackend::WasmGC | TargetBackend::Js);

    let build_opt = BuildPackageFlags {
        debug_flag,
        strip_flag: build_flags.strip(),
        source_map,
        enable_coverage,
        deny_warn: false,
        target_backend,
        warn_list: build_flags.warn_list.clone(),
        alert_list: build_flags.alert_list.clone(),
        enable_value_tracing: build_flags.enable_value_tracing,
    };

    let link_opt = LinkCoreFlags {
        debug_flag,
        source_map,
        output_format,
        target_backend,
    };

    let nostd = !build_flags.std() || moon_mod.name == MOONBITLANG_CORE;
    let render =
        !build_flags.no_render || std::env::var("MOON_NO_RENDER").unwrap_or_default() == "1";

    Ok(MooncOpt {
        build_opt,
        link_opt,
        extra_build_opt,
        extra_link_opt,
        nostd,
        render,
    })
}

#[test]
fn gen_docs_for_moon_help_page() {
    let markdown: String = clap_markdown::help_markdown::<MoonBuildSubcommands>();
    let markdown = markdown.replace("Default value: `zsh`", "Default value: `<your shell>`");
    let markdown = markdown.replace("Default value: `bash`", "Default value: `<your shell>`");
    let markdown = markdown.replace("Default value: `fish`", "Default value: `<your shell>`");
    let markdown = markdown.replace(
        "Default value: `powershell`",
        "Default value: `<your shell>`",
    );
    let mut lines = Vec::new();
    let mut need_trim = false;
    for line in markdown.lines() {
        if line.starts_with("## `moon shell-completion`") {
            need_trim = true;
        }
        if need_trim {
            if let Some(stripped) = line.strip_prefix("    ") {
                lines.push(stripped)
            } else {
                lines.push(line)
            }
        } else {
            lines.push(line);
        }
        if line.starts_with("  Possible values:") {
            need_trim = false;
        }
    }
    let markdown = lines.join("\n");
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let file_path =
        std::path::PathBuf::from(&manifest_dir).join("../../docs/manual-zh/src/commands.md");
    expect_test::expect_file!(file_path).assert_eq(&markdown);
    let file_path =
        std::path::PathBuf::from(&manifest_dir).join("../../docs/manual/src/commands.md");
    expect_test::expect_file!(file_path).assert_eq(&markdown);
}
