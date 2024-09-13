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

use anyhow::bail;
use moonbuild::dry_run::print_commands;
use mooncake::pkg::sync::auto_sync;
use moonutil::common::{
    read_module_desc_file_in_dir, CargoPathExt, FileLock, MoonbuildOpt, MooncOpt, RunMode,
    MOONBITLANG_CORE,
};
use moonutil::dirs::{mk_arch_mode_dir, PackageDirs};
use moonutil::mooncakes::sync::AutoSyncFlags;
use moonutil::mooncakes::RegistryConfig;

use super::UniversalFlags;

/// Generate documentation
#[derive(Debug, clap::Parser)]
pub struct DocSubcommand {
    /// Start a web server to serve the documentation
    #[clap(long)]
    pub serve: bool,

    /// The address of the server
    #[clap(long, short, default_value = "127.0.0.1", requires("serve"))]
    pub bind: String,

    /// The port of the server
    #[clap(long, short, default_value = "3000", requires("serve"))]
    pub port: u16,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,
}

pub fn run_doc(cli: UniversalFlags, cmd: DocSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    let static_dir = target_dir.join("doc");
    if !static_dir.exists() {
        std::fs::create_dir_all(&static_dir)?;
    }
    let _lock = FileLock::lock(&static_dir)?;

    if static_dir.exists() {
        static_dir.rm_rf();
    }
    let serve = cmd.serve;
    let bind = cmd.bind;
    let port = cmd.port;

    let mod_desc = read_module_desc_file_in_dir(&source_dir)?;

    let mut moonc_opt = MooncOpt::default();
    if mod_desc.name == MOONBITLANG_CORE {
        moonc_opt.nostd = true;
    }

    let (resolved_env, dir_sync_result) = auto_sync(
        &source_dir,
        &cmd.auto_sync_flags,
        &RegistryConfig::load(),
        cli.quiet,
    )?;

    let run_mode = RunMode::Check;
    let target_dir = mk_arch_mode_dir(&source_dir, &target_dir, &moonc_opt, run_mode)?;
    let moonbuild_opt = MoonbuildOpt {
        source_dir: source_dir.clone(),
        target_dir,
        sort_input: true,
        run_mode,
        ..Default::default()
    };

    let module = moonutil::scan::scan(
        false,
        &resolved_env,
        &dir_sync_result,
        &moonc_opt,
        &moonbuild_opt,
    )?;

    let mut args = vec![
        source_dir.display().to_string(),
        "-o".to_string(),
        static_dir.display().to_string(),
        "-std-path".to_string(),
        moonutil::moon_dir::core_bundle(moonc_opt.link_opt.target_backend)
            .display()
            .to_string(),
        "-packages-json".to_string(),
        moonbuild_opt
            .target_dir
            .join("packages.json")
            .display()
            .to_string(),
    ];
    if serve {
        args.push("-serve-mode".to_string())
    }
    if cli.dry_run {
        print_commands(&module, &moonc_opt, &moonbuild_opt)?;
        println!("moondoc {}", args.join(" "));
        return Ok(0);
    }
    moonbuild::entry::run_check(&moonc_opt, &moonbuild_opt, &module)?;
    let output = std::process::Command::new("moondoc").args(&args).output()?;
    if output.status.code().unwrap() != 0 {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        bail!("failed to generate documentation");
    }

    if serve {
        moonbuild::doc_http::start_server(static_dir, &mod_desc.name, bind, port)?;
    }
    Ok(0)
}
