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

//! Command parsing, selection, and orchestration. Only `execute` is exposed to
//! the rest of the binary; command definitions and planning adapters stay here.

mod bench;
mod build;
mod build_flags;
mod build_matrix;
mod bundle;
mod check;
mod clean;
mod coverage;
mod cram;
mod deps;
mod doc;
mod explain;
mod external;
mod fetch;
mod fix;
mod fmt;
mod generate_test_driver;
mod generated_mbti;
mod info;
mod install_binary;
mod invocation;
mod mooncake_adapter;
mod moonx;
mod new;
mod process;
mod profile;
mod prove;
#[cfg(test)]
mod registry_output_tests;
mod registry_queries;
mod registry_runner;
mod run;
mod runtime;
mod search;
mod selection;
mod shell_completion;
mod test;
mod testcli;
#[cfg(test)]
mod tests;
mod tool;
mod tree;
mod update;
mod upgrade;
mod version;
mod view;
mod whoami;
mod work;
use build_flags::BuildFlags;
pub(crate) use runtime::execute;

use bench::BenchSubcommand;
use build::BuildSubcommand;
use build_matrix::GenerateBuildMatrix;
use bundle::BundleSubcommand;
use check::CheckSubcommand;
use clean::CleanSubcommand;
use coverage::CoverageSubcommand;
use cram::CramSubcommand;
use deps::{InstallSubcommand, RunPostaddSubcommand};
use doc::DocSubcommand;
use explain::ExplainSubcommand;
use fetch::FetchSubcommand;
use fix::FixSubcommand;
use fmt::FmtSubcommand;
use generate_test_driver::GenerateTestDriverSubcommand;
use info::InfoSubcommand;
use mooncake::cli::{
    DeprecateSubcommand, LoginSubcommand, PackageSubcommand, PublishSubcommand, RegisterSubcommand,
};
use mooncake::pkg::{add::AddSubcommand, remove::RemoveSubcommand};
use new::NewSubcommand;
use prove::ProveSubcommand;
use run::RunSubcommand;
use search::SearchSubcommand;
use shell_completion::ShellCompSubCommand;
use test::TestSubcommand;
use tool::ToolSubcommand;
use tree::TreeSubcommand;
use update::UpdateSubcommand;
use upgrade::UpgradeSubcommand;
use version::VersionSubcommand;
use view::ViewSubcommand;
use whoami::WhoamiSubcommand;
use work::WorkSubcommand;

fn standalone_mbtx_path<'a>(
    paths: &'a [std::path::PathBuf],
    command: &str,
) -> anyhow::Result<Option<&'a std::path::Path>> {
    let Some(path) = paths.iter().find(|path| {
        path.extension()
            .is_some_and(|extension| extension == "mbtx")
            && path.is_file()
    }) else {
        return Ok(None);
    };

    anyhow::ensure!(
        paths.len() == 1,
        "standalone `.mbtx` `{command}` expects exactly one `PATH`"
    );
    Ok(Some(path))
}

#[derive(Debug, clap::Parser, serde::Serialize, Clone)]
#[clap(next_help_heading = "Common Options")]
struct UniversalFlags {
    #[clap(flatten)]
    source_tgt_dir: moonutil::project::SourceTargetDirs,

    #[clap(skip)]
    #[serde(skip)]
    workspace_env: moonutil::project::WorkspaceEnv,

    /// Suppress output
    #[clap(long, short = 'q', global = true)]
    quiet: bool,

    /// Increase verbosity
    #[clap(long, short = 'v', global = true)]
    verbose: bool,

    /// Trace the execution of the program
    #[clap(long, global = true)]
    trace: bool,

    /// Do not actually run the command
    #[clap(long, global = true)]
    dry_run: bool,

    /// Unstable flags to MoonBuild.
    #[clap(long, short = 'Z', default_value = "", env = "MOON_UNSTABLE")]
    unstable_feature: Box<moonutil::features::FeatureGate>,
}

impl UniversalFlags {
    fn user_log_level(&self) -> log::LevelFilter {
        moonutil::user_log::user_log_level(self.verbose, self.quiet)
    }
}

#[derive(Debug, clap::Parser)]
#[clap(
    name = "moon",
    about = "The build system and package manager for MoonBit.",
    override_usage = "moon [OPTIONS] <COMMAND>"
)]
struct MoonBuildCli {
    #[clap(subcommand)]
    pub subcommand: Option<MoonBuildSubcommands>,

    /// Print all version information and exit
    #[clap(short = 'V', long)]
    pub version: bool,

    #[clap(flatten)]
    pub flags: UniversalFlags,
}

#[derive(Debug, clap::Parser)]
enum MoonBuildSubcommands {
    New(NewSubcommand),

    // Build system
    Bundle(BundleSubcommand),
    Build(BuildSubcommand),
    Check(CheckSubcommand),
    Fix(FixSubcommand),
    Prove(ProveSubcommand),
    Run(RunSubcommand),
    Test(TestSubcommand),
    /// Run CLI tests with the current toolchain's mooncram
    ///
    /// Executes `moonrun <toolchain>/bin/mooncram.wasm -- [ARGS]...` without
    /// loading a MoonBit project. All arguments after `testcli`, including
    /// `--help`, `--version`, and `--`, are forwarded unchanged.
    ///
    /// Uses the current toolchain, including MOON_TOOLCHAIN_ROOT and
    /// MOONRUN_OVERRIDE. Moon options such as `-C` must appear before `testcli`;
    /// `--trace` is not supported before this command.
    // Invocation selection retains this opaque tail before Clap runs. The
    // argument declaration supplies help and completion metadata only.
    #[clap(
        name = "testcli",
        arg = clap::Arg::new("args")
            .value_name("ARGS")
            .num_args(0..)
            .help("Arguments forwarded to mooncram")
    )]
    TestCli,
    #[clap(hide = true)]
    Cram(CramSubcommand),
    #[clap(hide = true)]
    GenerateTestDriver(GenerateTestDriverSubcommand),
    Clean(CleanSubcommand),
    Fmt(FmtSubcommand),
    Doc(DocSubcommand),
    Explain(ExplainSubcommand),
    Info(InfoSubcommand),
    Bench(BenchSubcommand),

    // Dependencies
    Add(AddSubcommand),
    Remove(RemoveSubcommand),
    Install(InstallSubcommand),
    Tree(TreeSubcommand),
    Fetch(FetchSubcommand),
    RunPostadd(RunPostaddSubcommand),
    Search(SearchSubcommand),
    View(ViewSubcommand),
    Work(WorkSubcommand),

    // Mooncake
    Login(LoginSubcommand),
    Whoami(WhoamiSubcommand),
    Register(RegisterSubcommand),
    Publish(PublishSubcommand),
    Package(PackageSubcommand),
    Deprecate(DeprecateSubcommand),

    Update(UpdateSubcommand),

    // Misc
    Coverage(CoverageSubcommand),
    GenerateBuildMatrix(GenerateBuildMatrix),
    /// Upgrade toolchains
    Upgrade(UpgradeSubcommand),
    ShellCompletion(ShellCompSubCommand),
    Version(VersionSubcommand),
    #[clap(hide = true)]
    Tool(ToolSubcommand),

    // External subcommands
    #[clap(external_subcommand)]
    External(Vec<String>),
}
#[test]
fn gen_docs_for_moon_help_page() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let error_codes_dir = std::path::PathBuf::from(&manifest_dir)
        .join("../../crates/moonutil/resources/error_codes/language/error_codes");
    let has_error_code_docs = error_codes_dir
        .read_dir()
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .any(|entry| entry.path().extension().is_some_and(|ext| ext == "md"));
    assert!(
        has_error_code_docs,
        "missing MoonBit docs submodule at {}. Run `git submodule update --init --recursive` before running tests or promoting snapshots.",
        error_codes_dir.display()
    );

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
    let file_path =
        std::path::PathBuf::from(&manifest_dir).join("../../docs/manual-zh/src/commands.md");
    expect_test::expect_file!(file_path).assert_eq(&markdown);
    let file_path =
        std::path::PathBuf::from(&manifest_dir).join("../../docs/manual/src/commands.md");
    expect_test::expect_file!(file_path).assert_eq(&markdown);
}
