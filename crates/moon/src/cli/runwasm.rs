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

use std::path::{Path, PathBuf};

use anyhow::bail;
use mooncake::registry::RegistryClient;
use moonutil::{
    cli_support::AutoSyncFlags, cli_support::UniversalFlags, command_output::CommandOutput,
    constants::is_moon_pkg_exist, target::SurfaceTarget, user_log::UserLog,
};
use tracing::instrument;

use super::{BuildFlags, RunSubcommand, registry_runner::ResolvedExecutablePackage};
use crate::cli::process::ProcessAction;

/// Run a local package as WebAssembly or a prebuilt WebAssembly binary
#[derive(Debug, clap::Parser)]
#[clap(
    long_about = r#"Run a local package as WebAssembly or a prebuilt WebAssembly binary published as a Mooncakes asset.

Local package inputs are handled like `moon run --target wasm`:
  moon runwasm main
  moon runwasm ./main

Experimental moonrun policy forwarding:
  moon runwasm --experimental-policy moonrun-policy.json main

Accepted Mooncakes coordinate forms:
  moon runwasm moonbitlang/parser/cmd/moonfmt@0.3.3
  moon runwasm moonbitlang/parser/cmd/moonfmt@latest
  moon runwasm moonbitlang/parser/cmd/moonfmt

Pinned coordinates use the given version directly. `@latest` refreshes the
registry index before resolving the latest version. Unpinned coordinates use
the latest version already in the local index, updating it only when the module
is absent. Fetched wasm files are cached under $MOON_HOME/registry/cache/assets
and reused on later runs."#
)]
pub(crate) struct RunWasmSubcommand {
    /// Local package path or Mooncakes package coordinate of the prebuilt wasm binary
    #[clap(value_name = "LOCAL_PACKAGE|PACKAGE[@VERSION]")]
    pub package: String,

    /// Experimental: pass a moonrun JSON policy file for moonbitlang/async runtime access.
    ///
    /// The policy applies to moonbitlang/async and moonrun-owned unstable FFI;
    /// WASI is not covered.
    #[clap(long = "experimental-policy", value_name = "PATH")]
    pub experimental_policy: Option<PathBuf>,

    /// The arguments provided to the wasm program
    #[clap(trailing_var_arg = true, num_args = 0.., allow_hyphen_values = true)]
    pub args: Vec<String>,
}

#[instrument(skip_all)]
pub(crate) fn run_runwasm(
    cli: &UniversalFlags,
    cmd: RunWasmSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<ProcessAction> {
    if should_run_as_local_package(&cmd.package)? {
        return super::run_run(cli, runwasm_as_run_subcommand(cmd), output)
            .map(ProcessAction::Exit);
    }

    if cli.dry_run {
        bail!("--dry-run is not supported for Mooncakes assets in `moon runwasm`");
    }
    super::registry_runner::prepare(
        cmd.package,
        super::registry_runner::RegistryRunTarget::Wasm {
            experimental_policy: cmd.experimental_policy,
        },
        cmd.args,
        cli.quiet,
        cli.verbose,
        output.user_log(),
    )
    .map(ProcessAction::Delegate)
}

pub(super) fn cached_wasm_path(
    package: &ResolvedExecutablePackage,
    user_log: &UserLog,
) -> anyhow::Result<PathBuf> {
    RegistryClient::configured().acquire_wasm_asset(
        &package.module_name,
        &package.version,
        &package.package_path,
        user_log,
    )
}

fn should_run_as_local_package(input: &str) -> anyhow::Result<bool> {
    let path = Path::new(input);
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return Ok(false),
    };
    if metadata.is_dir() && is_moon_pkg_exist(path) {
        return Ok(true);
    }

    bail!("`{input}` is not a package directory")
}

fn runwasm_as_run_subcommand(cmd: RunWasmSubcommand) -> RunSubcommand {
    let build_flags = BuildFlags {
        target: vec![SurfaceTarget::Wasm],
        ..BuildFlags::default()
    };
    RunSubcommand {
        package_or_mbt_file: Some(cmd.package),
        command: None,
        build_flags,
        args: cmd.args,
        moonrun_policy: cmd.experimental_policy,
        auto_sync_flags: AutoSyncFlags { frozen: false },
        build_only: false,
        profile: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_package_paths_are_run_locally() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("moon.pkg.json"), "{}").unwrap();

        assert!(should_run_as_local_package(dir.path().to_str().unwrap()).unwrap());
    }

    #[test]
    fn existing_non_package_paths_are_rejected() {
        let dir = tempfile::TempDir::new().unwrap();
        let wasm = dir.path().join("main.wasm");
        std::fs::write(&wasm, b"\0asmtest").unwrap();

        let err = should_run_as_local_package(wasm.to_str().unwrap()).unwrap_err();
        assert_eq!(
            err.to_string(),
            format!("`{}` is not a package directory", wasm.to_string_lossy())
        );
    }

    #[test]
    fn mooncakes_coordinates_use_remote_asset_path() {
        assert!(!should_run_as_local_package("moonbitlang/parser/cmd/moonfmt@0.3.3").unwrap());
        assert!(!should_run_as_local_package("moonbitlang/parser/cmd/moonfmt").unwrap());
        assert!(!should_run_as_local_package("missing.mbt").unwrap());
        assert!(!should_run_as_local_package("missing.mbtx").unwrap());
        assert!(!should_run_as_local_package("missing.wasm").unwrap());
    }
}
