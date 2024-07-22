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
pub mod run;
pub mod test;
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
pub use new::*;
pub use run::*;
pub use test::*;
pub use update::*;
pub use upgrade::*;
pub use version::*;

use std::path::Path;

use anyhow::bail;
use moonutil::{
    cli::UniversalFlags,
    common::{
        read_module_desc_file_in_dir, BuildPackageFlags, LinkCoreFlags, MooncOpt, OutputFormat,
        SurfaceTarget, TargetBackend, MOONBITLANG_CORE, MOON_MOD_JSON,
    },
    mooncakes::{LoginSubcommand, PublishSubcommand, RegisterSubcommand},
};

#[derive(Debug, clap::Parser)]
#[clap(name = "moon", about = "MoonBit's build system")]
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
    GenerateTestDriver(GeneratedTestDriverSubcommand),
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

    Update(UpdateSubcommand),

    // Misc
    Coverage(CoverageSubcommand),
    GenerateBuildMatrix(GenerateBuildMatrix),
    /// Upgrade toolchains
    Upgrade,
    Version(VersionSubcommand),
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

    /// Select output target
    #[clap(long, value_delimiter = ',')]
    pub target: Option<Vec<SurfaceTarget>>,

    #[clap(skip)]
    pub target_backend: Option<TargetBackend>,

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

    /// treat all warnings as errors
    #[clap(long, short)]
    pub deny_warn: bool,

    /// don't render diagnostics from moonc (don't pass '-error-format json' to moonc)
    #[clap(long)]
    pub no_render: bool,
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

    let output_format = if target_backend == TargetBackend::Js {
        OutputFormat::Js
    } else {
        output_format
    };

    let debug_flag = build_flags.debug;
    let enable_coverage = build_flags.enable_coverage;
    let source_map =
        debug_flag && matches!(target_backend, TargetBackend::WasmGC | TargetBackend::Js);

    let build_opt = BuildPackageFlags {
        debug_flag,
        source_map,
        enable_coverage,
        warn_lists: Default::default(),
        alert_lists: Default::default(),
        deny_warn: false,
        target_backend,
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
