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

pub(crate) mod bench;
pub(crate) mod build;
pub(crate) mod build_matrix;
pub(crate) mod bundle;
pub(crate) mod check;
pub(crate) mod clean;
pub(crate) mod coverage;
pub(crate) mod deps;
pub(crate) mod doc;
pub(crate) mod external;
pub(crate) mod fetch;
pub(crate) mod fmt;
pub(crate) mod generate_test_driver;
pub(crate) mod info;
pub(crate) mod install_binary;
pub(crate) mod mooncake_adapter;
pub(crate) mod new;
pub(crate) mod prove;
pub(crate) mod query;
pub(crate) mod run;
pub(crate) mod shell_completion;
pub(crate) mod test;
pub(crate) mod tool;
pub(crate) mod update;
pub(crate) mod upgrade;
pub(crate) mod version;
pub(crate) mod whoami;
mod work;
pub(crate) use crate::build_flags::BuildFlags;
pub(crate) use bench::*;
pub(crate) use build::*;
pub(crate) use build_matrix::*;
pub(crate) use bundle::*;
pub(crate) use check::*;
pub(crate) use clean::*;
pub(crate) use coverage::*;
pub(crate) use deps::*;
pub(crate) use doc::*;
pub(crate) use external::*;
pub(crate) use fetch::*;
pub(crate) use fmt::*;
pub(crate) use generate_test_driver::*;
pub(crate) use info::*;
use moonbuild::upgrade::UpgradeSubcommand;
use mooncake::pkg::{
    add::AddSubcommand, install::InstallSubcommand, remove::RemoveSubcommand, tree::TreeSubcommand,
};
use moonutil::{
    cli::UniversalFlags,
    mooncakes::{LoginSubcommand, PackageSubcommand, PublishSubcommand, RegisterSubcommand},
};
pub(crate) use new::*;
pub(crate) use prove::*;
pub(crate) use query::*;
pub(crate) use run::*;
pub(crate) use shell_completion::*;
pub(crate) use test::*;
pub(crate) use tool::*;
pub(crate) use update::*;
pub(crate) use upgrade::*;
pub(crate) use version::*;
pub(crate) use whoami::*;
pub(crate) use work::{WorkSubcommand, work_cli};
#[derive(Debug, clap::Parser)]
#[clap(
    name = "moon",
    about = "The build system and package manager for MoonBit."
)]
pub(crate) struct MoonBuildCli {
    #[clap(subcommand)]
    pub subcommand: MoonBuildSubcommands,

    #[clap(flatten)]
    pub flags: UniversalFlags,
}

#[derive(Debug, clap::Parser)]
pub(crate) enum MoonBuildSubcommands {
    New(NewSubcommand),

    // Build system
    Bundle(BundleSubcommand),
    Build(BuildSubcommand),
    Check(CheckSubcommand),
    Prove(ProveSubcommand),
    Run(RunSubcommand),
    Test(TestSubcommand),
    #[clap(hide = true)]
    GenerateTestDriver(GenerateTestDriverSubcommand),
    Clean(CleanSubcommand),
    Fmt(FmtSubcommand),
    Doc(DocSubcommand),
    Info(InfoSubcommand),
    Bench(BenchSubcommand),

    // Dependencies
    Add(AddSubcommand),
    Remove(RemoveSubcommand),
    Install(InstallSubcommand),
    Tree(TreeSubcommand),
    Fetch(FetchSubcommand),
    Work(WorkSubcommand),

    // Mooncake
    Login(LoginSubcommand),
    Whoami(WhoamiSubcommand),
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

    // External subcommands
    #[clap(external_subcommand)]
    External(Vec<String>),
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
