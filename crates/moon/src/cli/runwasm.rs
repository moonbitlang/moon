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

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use moonbuild_rupes_recta::model::RunBackend;
use mooncake::registry::{OnlineRegistry, Registry, path as registry_path};
use moonutil::{
    cli::UniversalFlags,
    common::{FileLock, SurfaceTarget},
    mooncakes::sync::AutoSyncFlags,
    mooncakes::{ModuleName, RegistryConfig},
};
use reqwest::{StatusCode, header::USER_AGENT};
use semver::Version;
use tracing::instrument;

use super::{BuildFlags, RunSubcommand};
use crate::{rr_build, run::default_rt, user_diagnostics::UserDiagnostics};

/// Run a local target as WebAssembly or a prebuilt WebAssembly binary
#[derive(Debug, clap::Parser)]
#[clap(
    long_about = r#"Run a local target as WebAssembly or a prebuilt WebAssembly binary published as a Mooncakes asset.

Local inputs are handled like `moon run --target wasm`:
  moon runwasm main
  moon runwasm ./main/main.mbt
  moon runwasm ./target/main.wasm

Accepted Mooncakes coordinate forms:
  moon runwasm moonbitlang/parser/cmd/moonfmt@0.3.3
  moon runwasm moonbitlang/parser@0.3.3/cmd/moonfmt
  moon runwasm moonbitlang/parser/cmd/moonfmt

Pinned coordinates use the given version directly. Unpinned coordinates resolve
the latest version from the registry index. Fetched wasm files are cached under
$MOON_HOME/registry/cache/assets and reused on later runs."#
)]
pub(crate) struct RunWasmSubcommand {
    /// Local package/file path or Mooncakes package coordinate of the prebuilt wasm binary
    #[clap(value_name = "PATH|PACKAGE[@VERSION]")]
    pub package: String,

    /// The arguments provided to the wasm program
    #[clap(trailing_var_arg = true, num_args = 0.., allow_hyphen_values = true)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunWasmCoordinate {
    module_name: ModuleName,
    package_path: String,
    version: Option<Version>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedRunWasmAsset {
    url: String,
    cache_path: PathBuf,
}

impl RunWasmCoordinate {
    fn binary_name(&self) -> String {
        if self.package_path.is_empty() {
            self.module_name.last_segment().to_string()
        } else {
            self.package_path
                .rsplit('/')
                .next()
                .expect("non-empty package path must have a last segment")
                .to_string()
        }
    }

    fn with_version(self, version: Version, registry: &str) -> ResolvedRunWasmAsset {
        let binary_name = self.binary_name();
        let url = asset_url(
            registry,
            &self.module_name,
            &self.package_path,
            &version,
            &binary_name,
        );
        let cache_path = asset_cache_path(
            &self.module_name,
            &self.package_path,
            &version,
            &binary_name,
        );
        ResolvedRunWasmAsset { url, cache_path }
    }
}

#[instrument(skip_all)]
pub(crate) fn run_runwasm(cli: &UniversalFlags, cmd: RunWasmSubcommand) -> anyhow::Result<i32> {
    if should_run_as_local_input(&cmd.package) {
        return super::run_run(cli, runwasm_as_run_subcommand(cmd));
    }

    if cli.dry_run {
        bail!("--dry-run is not supported for Mooncakes assets in `moon runwasm`");
    }
    let output = UserDiagnostics::from_flags(cli);
    let coordinate = parse_runwasm_coordinate(&cmd.package)?;
    let registry_config = RegistryConfig::load();
    let version = match coordinate.version.clone() {
        Some(version) => version,
        None => resolve_latest_version(cli, &coordinate.module_name, output)?,
    };
    let asset = coordinate.with_version(version, &registry_config.registry);
    let wasm_path = ensure_cached_wasm(&asset, output)?;

    let mut run_cmd = crate::run::command_for(RunBackend::WasmGC, None, &wasm_path, None);
    run_cmd.args(&cmd.args);

    if cli.verbose {
        let print_dir = std::env::current_dir().context("failed to get current directory")?;
        rr_build::dry_print_command(run_cmd.as_std(), &print_dir, true);
    }

    let res = default_rt()
        .context("Failed to create runtime")?
        .block_on(crate::run::run(&mut [], false, run_cmd))
        .context("failed to run command")?;

    if crate::run::shutdown_requested() {
        return Ok(130);
    }

    if let Some(code) = res.code() {
        Ok(code)
    } else {
        bail!("Command exited without a return code")
    }
}

fn should_run_as_local_input(input: &str) -> bool {
    let path = Path::new(input);
    std::fs::metadata(path).is_ok()
        || path.is_absolute()
        || matches!(input, "." | "..")
        || input.starts_with("./")
        || input.starts_with("../")
        || path
            .extension()
            .is_some_and(|extension| matches!(extension.to_str(), Some("mbt" | "mbtx" | "wasm")))
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
        auto_sync_flags: AutoSyncFlags { frozen: false },
        build_only: false,
        profile: false,
    }
}

fn resolve_latest_version(
    cli: &UniversalFlags,
    module_name: &ModuleName,
    output: UserDiagnostics,
) -> anyhow::Result<Version> {
    let index_dir = moonutil::moon_dir::index();
    let registry_config = RegistryConfig::load();
    let had_index = index_dir.exists();

    match mooncake::update::update_with_output(
        &index_dir,
        &registry_config,
        mooncake::update::UpdateOutput::Quiet,
    ) {
        Ok(_) => output.info("Updated registry index"),
        Err(e) => {
            if had_index {
                output.warn(format!(
                    "Failed to update registry index, using cached index: {}",
                    e
                ));
            } else {
                return Err(e).context("Failed to update registry index");
            }
        }
    }

    let registry = OnlineRegistry::mooncakes_io();
    let module_info = registry.get_latest_version(module_name).ok_or_else(|| {
        if had_index {
            anyhow::anyhow!("Module `{module_name}` not found in registry")
        } else {
            anyhow::anyhow!("Module `{module_name}` not found in registry after updating the index")
        }
    })?;
    let version = module_info
        .version
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Module `{module_name}` has no version information"))?;
    if !cli.quiet {
        output.info(format!(
            "Resolved {module_name} latest version to {version}"
        ));
    }
    Ok(version)
}

fn ensure_cached_wasm(
    asset: &ResolvedRunWasmAsset,
    output: UserDiagnostics,
) -> anyhow::Result<PathBuf> {
    ensure_cached_wasm_with(asset, output, download_wasm)
}

fn ensure_cached_wasm_with(
    asset: &ResolvedRunWasmAsset,
    output: UserDiagnostics,
    download: impl FnOnce(&str) -> anyhow::Result<Vec<u8>>,
) -> anyhow::Result<PathBuf> {
    if asset.cache_path.exists() {
        output.info(format!(
            "Using cached {}",
            asset.cache_path.to_string_lossy()
        ));
        return Ok(asset.cache_path.clone());
    }

    let parent = asset
        .cache_path
        .parent()
        .context("runwasm cache path has no parent")?;
    std::fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create runwasm cache directory {}",
            parent.display()
        )
    })?;
    let _lock = FileLock::lock(parent)
        .with_context(|| format!("failed to lock cache directory {}", parent.display()))?;

    if asset.cache_path.exists() {
        output.info(format!(
            "Using cached {}",
            asset.cache_path.to_string_lossy()
        ));
        return Ok(asset.cache_path.clone());
    }

    output.info(format!("Downloading {}", asset.url));
    let bytes = download(&asset.url)?;

    let tmp_path = asset.cache_path.with_extension("wasm.download");
    let mut tmp = std::fs::File::create(&tmp_path)
        .with_context(|| format!("failed to create download file {}", tmp_path.display()))?;
    tmp.write_all(&bytes)
        .context("failed to write downloaded wasm to cache")?;
    std::fs::rename(&tmp_path, &asset.cache_path).with_context(|| {
        format!(
            "failed to move downloaded wasm from {} to {}",
            tmp_path.display(),
            asset.cache_path.display()
        )
    })?;

    Ok(asset.cache_path.clone())
}

fn download_wasm(url: &str) -> anyhow::Result<Vec<u8>> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(url)
        .header(USER_AGENT, format!("moon/{}", env!("CARGO_PKG_VERSION")))
        .send()
        .with_context(|| format!("failed to download prebuilt wasm from {url}"))?;
    report_missing_asset_for_404(response.status())?;
    let data = response
        .error_for_status()
        .with_context(|| format!("prebuilt wasm download returned error status for {url}"))?
        .bytes()
        .with_context(|| format!("failed to read prebuilt wasm response from {url}"))?;
    Ok(data.to_vec())
}

fn report_missing_asset_for_404(status: StatusCode) -> anyhow::Result<()> {
    if status == StatusCode::NOT_FOUND {
        bail!("Prebuilt wasm asset does not exist");
    }
    Ok(())
}

fn parse_runwasm_coordinate(input: &str) -> anyhow::Result<RunWasmCoordinate> {
    if input.matches('@').count() > 1 {
        bail!("Invalid runwasm coordinate `{input}`: multiple `@` version markers found");
    }
    if input.ends_with("...") {
        bail!("Invalid runwasm coordinate `{input}`: wildcard package paths are not supported");
    }

    if input.contains('@') {
        let parsed = if let Ok(parsed) = registry_path::parse_module_at_version_path(input) {
            parsed
        } else if let Ok(parsed) = registry_path::parse_package_at_version_path(input) {
            parsed
        } else {
            bail!("Invalid runwasm coordinate `{input}`");
        };
        validate_components(input, &parsed.full_path_without_version(), "package")?;
        let version = Version::parse(&parsed.version).with_context(|| {
            format!("Invalid version `{}` in runwasm coordinate", parsed.version)
        })?;
        return Ok(RunWasmCoordinate {
            module_name: parsed.module,
            package_path: parsed.package,
            version: Some(version),
        });
    }

    validate_components(input, input, "package")?;
    let parsed = registry_path::parse_install_style_path(input)
        .with_context(|| format!("Invalid runwasm coordinate `{input}`"))?;
    Ok(RunWasmCoordinate {
        module_name: parsed.module,
        package_path: parsed.package,
        version: None,
    })
}

fn validate_components(input: &str, path: &str, label: &str) -> anyhow::Result<()> {
    if path.is_empty()
        || path.split('/').any(|component| {
            component.is_empty() || component == "." || component == ".." || component.contains(':')
        })
    {
        bail!("Invalid runwasm coordinate `{input}`: invalid {label} path component");
    }
    Ok(())
}

fn asset_url(
    registry: &str,
    module_name: &ModuleName,
    package_path: &str,
    version: &Version,
    binary_name: &str,
) -> String {
    let base = registry.trim_end_matches('/');
    let module = module_name.to_string();
    if package_path.is_empty() {
        format!("{base}/assets/{module}@{version}/{binary_name}.wasm")
    } else {
        format!("{base}/assets/{module}@{version}/{package_path}/{binary_name}.wasm")
    }
}

fn asset_cache_path(
    module_name: &ModuleName,
    package_path: &str,
    version: &Version,
    binary_name: &str,
) -> PathBuf {
    let mut path = moonutil::moon_dir::cache()
        .join("assets")
        .join(module_name.username.as_str());
    for segment in module_name.unqual.split('/') {
        path.push(segment);
    }
    path.push(version.to_string());
    for segment in package_path
        .split('/')
        .filter(|segment| !segment.is_empty())
    {
        path.push(segment);
    }
    path.join(format!("{binary_name}.wasm"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> RunWasmCoordinate {
        parse_runwasm_coordinate(input).unwrap()
    }

    #[test]
    fn local_paths_are_run_locally() {
        let dir = tempfile::TempDir::new().unwrap();
        let wasm = dir.path().join("main.wasm");
        std::fs::write(&wasm, b"\0asmtest").unwrap();
        let wasm_named_dir = dir.path().join("pkg.wasm");
        std::fs::create_dir(&wasm_named_dir).unwrap();

        assert!(should_run_as_local_input(wasm.to_str().unwrap()));
        assert!(should_run_as_local_input(wasm_named_dir.to_str().unwrap()));
        assert!(should_run_as_local_input("./main/main.mbt"));
        assert!(should_run_as_local_input("missing.mbtx"));
        assert!(should_run_as_local_input("missing.wasm"));
    }

    #[test]
    fn mooncakes_coordinates_use_remote_asset_path() {
        assert!(!should_run_as_local_input(
            "moonbitlang/parser/cmd/moonfmt@0.3.3"
        ));
        assert!(!should_run_as_local_input("moonbitlang/parser/cmd/moonfmt"));
    }

    #[test]
    fn parse_install_style_version() {
        let parsed = parse("moonbitlang/parser/cmd/moonfmt@0.3.3");
        assert_eq!(parsed.module_name.to_string(), "moonbitlang/parser");
        assert_eq!(parsed.package_path, "cmd/moonfmt");
        assert_eq!(parsed.version.as_ref().unwrap().to_string(), "0.3.3");
        assert_eq!(parsed.binary_name(), "moonfmt");
    }

    #[test]
    fn parse_module_version_alias() {
        let parsed = parse("moonbitlang/parser@0.3.3/cmd/moonfmt");
        assert_eq!(parsed.module_name.to_string(), "moonbitlang/parser");
        assert_eq!(parsed.package_path, "cmd/moonfmt");
        assert_eq!(parsed.version.as_ref().unwrap().to_string(), "0.3.3");
        assert_eq!(parsed.binary_name(), "moonfmt");
    }

    #[test]
    fn parse_latest_coordinate() {
        let parsed = parse("moonbitlang/parser/cmd/moonfmt");
        assert_eq!(parsed.module_name.to_string(), "moonbitlang/parser");
        assert_eq!(parsed.package_path, "cmd/moonfmt");
        assert_eq!(parsed.version, None);
    }

    #[test]
    fn parse_root_package_uses_module_last_segment_as_binary_name() {
        let parsed = parse("moonbitlang/parser@0.3.3");
        assert_eq!(parsed.module_name.to_string(), "moonbitlang/parser");
        assert_eq!(parsed.package_path, "");
        assert_eq!(parsed.binary_name(), "parser");
    }

    #[test]
    fn reject_invalid_coordinates() {
        assert!(parse_runwasm_coordinate("moonbitlang/parser@bad/cmd/moonfmt").is_err());
        assert!(parse_runwasm_coordinate("moonbitlang/parser/cmd/moonfmt@bad").is_err());
        assert!(parse_runwasm_coordinate("moonbitlang/parser@0.3.3/cmd@0.4.0").is_err());
        assert!(parse_runwasm_coordinate("moonbitlang/parser/0.3.3@0.4.0/cmd").is_err());
        assert!(parse_runwasm_coordinate("moonbitlang/parser/...").is_err());
        assert!(parse_runwasm_coordinate("moonbitlang/parser//cmd").is_err());
        assert!(parse_runwasm_coordinate("./moonbitlang/parser").is_err());
        assert!(parse_runwasm_coordinate("C:/moonbitlang/parser").is_err());
        assert!(parse_runwasm_coordinate("https://mooncakes.io/x").is_err());
    }

    #[test]
    fn build_asset_url() {
        let parsed = parse("moonbitlang/parser/cmd/moonfmt@0.3.3");
        let resolved = parsed.with_version("0.3.3".parse().unwrap(), "https://mooncakes.io/");
        assert_eq!(
            resolved.url,
            "https://mooncakes.io/assets/moonbitlang/parser@0.3.3/cmd/moonfmt/moonfmt.wasm"
        );
    }

    #[test]
    fn build_root_asset_url() {
        let parsed = parse("moonbitlang/parser@0.3.3");
        let resolved = parsed.with_version("0.3.3".parse().unwrap(), "https://mooncakes.io");
        assert_eq!(
            resolved.url,
            "https://mooncakes.io/assets/moonbitlang/parser@0.3.3/parser.wasm"
        );
    }

    #[test]
    fn cache_miss_uses_downloader_once_and_writes_file() {
        let cache_dir = tempfile::TempDir::new().unwrap();
        let parsed = parse("moonbitlang/parser/cmd/moonfmt@0.3.3");
        let mut asset = parsed.with_version("0.3.3".parse().unwrap(), "https://mooncakes.io");
        asset.cache_path = cache_dir.path().join("moonfmt.wasm");
        let path = ensure_cached_wasm_with(&asset, UserDiagnostics::default(), |url| {
            assert_eq!(
                url,
                "https://mooncakes.io/assets/moonbitlang/parser@0.3.3/cmd/moonfmt/moonfmt.wasm"
            );
            Ok(b"\0asmtest".to_vec())
        })
        .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"\0asmtest");
    }

    #[test]
    fn download_status_404_reports_missing_asset() {
        let err = report_missing_asset_for_404(StatusCode::NOT_FOUND)
            .unwrap_err()
            .to_string();
        assert_eq!(err, "Prebuilt wasm asset does not exist");
        assert!(report_missing_asset_for_404(StatusCode::INTERNAL_SERVER_ERROR).is_ok());
    }
}
