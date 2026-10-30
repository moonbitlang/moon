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

use clap::Parser;
use std::path::PathBuf;
mod bundle_template;
mod ci;
mod sync_docs;

#[derive(Debug, clap::Parser)]
struct Cli {
    #[clap(subcommand)]
    pub subcommand: Option<XSubcommands>,
}

#[derive(Debug, clap::Parser)]
enum XSubcommands {
    #[command(name = "sync-docs")]
    SyncDocs(SyncDocs),

    #[command(name = "bundle-template")]
    BundleTemplate(BundleTemplate),

    #[command(name = "ci")]
    Ci(Ci),
}

#[derive(Debug, clap::Parser)]
struct SyncDocs {
    #[arg(long)]
    moonbit_docs_dir: PathBuf,
}

#[derive(Debug, clap::Parser)]
struct BundleTemplate {}

#[derive(Debug, Clone, clap::Parser, Default)]
struct Ci {}

fn main() {
    let cli = Cli::parse();
    let code = match cli.subcommand {
        Some(XSubcommands::SyncDocs(t)) => sync_docs::run(&t.moonbit_docs_dir).map_or(1, |_| 0),
        Some(XSubcommands::BundleTemplate(_)) => bundle_template::run().map_or(1, |_| 0),
        Some(XSubcommands::Ci(t)) => ci::run(&t).map_or(1, |_| 0),
        None => ci::run(&Ci::default()).map_or(1, |_| 0),
    };
    std::process::exit(code);
}
