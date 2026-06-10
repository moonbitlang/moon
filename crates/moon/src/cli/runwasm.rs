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
    common::{FileLock, SurfaceTarget, is_moon_pkg_exist},
    mooncakes::sync::AutoSyncFlags,
    mooncakes::{ModuleName, RegistryConfig},
};
use reqwest::{StatusCode, header::USER_AGENT};
use semver::Version;
use sha2::{Digest, Sha256};
use tracing::instrument;

use super::{BuildFlags, RunSubcommand};
use crate::{rr_build, run::default_rt, user_diagnostics::UserDiagnostics};

/// Run a local package as WebAssembly or a prebuilt WebAssembly binary
#[derive(Debug, clap::Parser)]
#[clap(
    long_about = r#"Run a local package as WebAssembly or a prebuilt WebAssembly binary published as a Mooncakes asset.

Local package inputs are handled like `moon run --target wasm`:
  moon runwasm main
  moon runwasm ./main

Accepted Mooncakes coordinate forms:
  moon runwasm moonbitlang/parser/cmd/moonfmt@0.3.3
  moon runwasm moonbitlang/parser@0.3.3/cmd/moonfmt
  moon runwasm moonbitlang/parser/cmd/moonfmt

Pinned coordinates use the given version directly. Unpinned coordinates resolve
the latest version from the registry index, updating it only when the module is
absent from the local index. Fetched wasm files are cached under
$MOON_HOME/registry/cache/assets and reused on later runs."#
)]
pub(crate) struct RunWasmSubcommand {
    /// Local package path or Mooncakes package coordinate of the prebuilt wasm binary
    #[clap(value_name = "LOCAL_PACKAGE|PACKAGE[@VERSION]")]
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
    checksum_url: String,
    cache_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LatestVersionLookup {
    Found(Version),
    NoVersionInformation,
    NotFound,
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
        let base = registry.trim_end_matches('/');
        let module = self.module_name.to_string();
        let url = if self.package_path.is_empty() {
            format!("{base}/assets/{module}@{version}/{binary_name}.wasm")
        } else {
            format!(
                "{base}/assets/{module}@{version}/{}/{}.wasm",
                self.package_path, binary_name
            )
        };

        let mut cache_path = moonutil::moon_dir::cache()
            .join("assets")
            .join(self.module_name.username.as_str());
        for segment in self.module_name.unqual.split('/') {
            cache_path.push(segment);
        }
        cache_path.push(version.to_string());
        for segment in self
            .package_path
            .split('/')
            .filter(|segment| !segment.is_empty())
        {
            cache_path.push(segment);
        }
        cache_path.push(format!("{binary_name}.wasm"));

        let checksum_url = format!("{url}.sha256");
        ResolvedRunWasmAsset {
            url,
            checksum_url,
            cache_path,
        }
    }
}

#[instrument(skip_all)]
pub(crate) fn run_runwasm(cli: &UniversalFlags, cmd: RunWasmSubcommand) -> anyhow::Result<i32> {
    if should_run_as_local_package(&cmd.package)? {
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
        None => resolve_latest_version(&coordinate.module_name, output)?,
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
        auto_sync_flags: AutoSyncFlags { frozen: false },
        build_only: false,
        profile: false,
    }
}

fn resolve_latest_version(
    module_name: &ModuleName,
    output: UserDiagnostics,
) -> anyhow::Result<Version> {
    let index_dir = moonutil::moon_dir::index();
    let registry_config = RegistryConfig::load();
    let had_index = index_dir.exists();

    resolve_latest_version_with(
        module_name,
        output,
        had_index,
        || latest_version_from_local_registry(module_name),
        || {
            mooncake::update::update_with_output(
                &index_dir,
                &registry_config,
                mooncake::update::UpdateOutput::Quiet,
            )
            .map(|_| ())
        },
    )
}

fn latest_version_from_local_registry(module_name: &ModuleName) -> LatestVersionLookup {
    let registry = OnlineRegistry::mooncakes_io();
    let versions = match registry.all_versions_of(module_name) {
        Ok(versions) => versions,
        Err(_) => return LatestVersionLookup::NotFound,
    };
    versions
        .last_key_value()
        .map(|(version, _)| LatestVersionLookup::Found(version.clone()))
        .unwrap_or(LatestVersionLookup::NoVersionInformation)
}

fn resolve_latest_version_with(
    module_name: &ModuleName,
    output: UserDiagnostics,
    had_index: bool,
    mut lookup_latest_version: impl FnMut() -> LatestVersionLookup,
    mut update_registry: impl FnMut() -> anyhow::Result<()>,
) -> anyhow::Result<Version> {
    if let LatestVersionLookup::Found(version) = lookup_latest_version() {
        output.info(format!(
            "Resolved {module_name} latest version to {version}"
        ));
        return Ok(version);
    }

    match update_registry() {
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

    let version = match lookup_latest_version() {
        LatestVersionLookup::Found(version) => version,
        LatestVersionLookup::NoVersionInformation => {
            bail!("Module `{module_name}` has no version information")
        }
        LatestVersionLookup::NotFound if had_index => {
            bail!("Module `{module_name}` not found in registry")
        }
        LatestVersionLookup::NotFound => {
            bail!("Module `{module_name}` not found in registry after updating the index")
        }
    };
    output.info(format!(
        "Resolved {module_name} latest version to {version}"
    ));
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
    mut download: impl FnMut(&str) -> anyhow::Result<Vec<u8>>,
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
    // The lock covers the whole check/download/publish sequence. Waiters re-check
    // after acquiring it; an existing final wasm means another process finished.
    let _lock = FileLock::lock(parent)
        .with_context(|| format!("failed to lock cache directory {}", parent.display()))?;

    if asset.cache_path.exists() {
        output.info(format!(
            "Using cached {}",
            asset.cache_path.to_string_lossy()
        ));
        return Ok(asset.cache_path.clone());
    }

    let checksum_bytes = download(&asset.checksum_url)?;
    let expected_checksum = parse_sha256_checksum(&checksum_bytes)
        .with_context(|| format!("invalid SHA-256 checksum from {}", asset.checksum_url))?;
    output.info(format!("Downloading {}", asset.url));
    let bytes = download(&asset.url)?;
    let actual_checksum = sha256_hex(&bytes);
    if actual_checksum != expected_checksum {
        bail!(
            "prebuilt wasm checksum mismatch for {}: expected {}, got {}",
            asset.url,
            expected_checksum,
            actual_checksum
        );
    }

    write_atomic(&asset.cache_path, &bytes)?;

    Ok(asset.cache_path.clone())
}

fn parse_sha256_checksum(bytes: &[u8]) -> anyhow::Result<String> {
    let text = std::str::from_utf8(bytes).context("SHA-256 checksum is not valid UTF-8")?;
    let checksum = text
        .split_whitespace()
        .next()
        .context("SHA-256 checksum is empty")?;
    if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("SHA-256 checksum must be a 64-character hex digest");
    }
    Ok(checksum.to_ascii_lowercase())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = path.parent().context("cache path has no parent")?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create download file in {}", parent.display()))?;
    tmp.write_all(bytes)
        .with_context(|| format!("failed to write download file {}", tmp.path().display()))?;
    tmp.as_file()
        .sync_all()
        .with_context(|| format!("failed to sync download file {}", tmp.path().display()))?;
    tmp.persist(path).with_context(|| {
        let path = path.display();
        format!("failed to move downloaded file to {path}")
    })?;
    if let Ok(dir) = std::fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

fn download_wasm(url: &str) -> anyhow::Result<Vec<u8>> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(url)
        .header(USER_AGENT, format!("moon/{}", env!("CARGO_PKG_VERSION")))
        .send()
        .with_context(|| format!("failed to download prebuilt wasm from {url}"))?;
    if response.status() == StatusCode::NOT_FOUND {
        bail!("Prebuilt wasm asset does not exist");
    }
    let data = response
        .error_for_status()
        .with_context(|| format!("prebuilt wasm download returned error status for {url}"))?
        .bytes()
        .with_context(|| format!("failed to read prebuilt wasm response from {url}"))?;
    Ok(data.to_vec())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> RunWasmCoordinate {
        parse_runwasm_coordinate(input).unwrap()
    }

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
    fn latest_resolution_uses_local_registry_before_updating() {
        let module_name = "moonbitlang/parser".parse::<ModuleName>().unwrap();
        let mut update_called = false;

        let version = resolve_latest_version_with(
            &module_name,
            UserDiagnostics::default(),
            true,
            || LatestVersionLookup::Found("0.3.3".parse().unwrap()),
            || {
                update_called = true;
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(version.to_string(), "0.3.3");
        assert!(!update_called);
    }

    #[test]
    fn latest_resolution_updates_after_local_registry_miss() {
        let module_name = "moonbitlang/parser".parse::<ModuleName>().unwrap();
        let mut lookup_count = 0;
        let mut update_called = false;

        let version = resolve_latest_version_with(
            &module_name,
            UserDiagnostics::default(),
            true,
            || {
                lookup_count += 1;
                if lookup_count > 1 {
                    LatestVersionLookup::Found("0.3.3".parse().unwrap())
                } else {
                    LatestVersionLookup::NotFound
                }
            },
            || {
                update_called = true;
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(version.to_string(), "0.3.3");
        assert_eq!(lookup_count, 2);
        assert!(update_called);
    }

    #[test]
    fn latest_resolution_preserves_no_version_information_after_update() {
        let module_name = "moonbitlang/parser".parse::<ModuleName>().unwrap();
        let mut update_called = false;

        let err = resolve_latest_version_with(
            &module_name,
            UserDiagnostics::default(),
            true,
            || LatestVersionLookup::NoVersionInformation,
            || {
                update_called = true;
                Ok(())
            },
        )
        .unwrap_err();

        assert_eq!(
            err.to_string(),
            "Module `moonbitlang/parser` has no version information"
        );
        assert!(update_called);
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
        assert_eq!(
            resolved.checksum_url,
            "https://mooncakes.io/assets/moonbitlang/parser@0.3.3/cmd/moonfmt/moonfmt.wasm.sha256"
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
    fn parse_sha256sum_output() {
        let checksum = "ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789";
        assert_eq!(
            parse_sha256_checksum(format!("{checksum}  moonfmt.wasm\n").as_bytes()).unwrap(),
            checksum.to_ascii_lowercase()
        );
        assert!(parse_sha256_checksum(b"not-a-checksum").is_err());
        assert!(parse_sha256_checksum(b"").is_err());
    }

    #[test]
    fn cache_miss_downloads_checksum_and_writes_file() {
        let cache_dir = tempfile::TempDir::new().unwrap();
        let parsed = parse("moonbitlang/parser/cmd/moonfmt@0.3.3");
        let mut asset = parsed.with_version("0.3.3".parse().unwrap(), "https://mooncakes.io");
        asset.cache_path = cache_dir.path().join("moonfmt.wasm");
        let wasm = b"\0asmtest".to_vec();
        let checksum = sha256_hex(&wasm);
        let mut urls = Vec::new();
        let path = ensure_cached_wasm_with(&asset, UserDiagnostics::default(), |url| {
            urls.push(url.to_string());
            if url.ends_with(".sha256") {
                Ok(format!("{checksum}  moonfmt.wasm\n").into_bytes())
            } else {
                Ok(wasm.clone())
            }
        })
        .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"\0asmtest");
        assert!(!cache_dir.path().join("moonfmt.wasm.sha256").exists());
        assert_eq!(
            urls,
            [
                "https://mooncakes.io/assets/moonbitlang/parser@0.3.3/cmd/moonfmt/moonfmt.wasm.sha256",
                "https://mooncakes.io/assets/moonbitlang/parser@0.3.3/cmd/moonfmt/moonfmt.wasm",
            ]
        );
    }

    #[test]
    fn cache_hit_uses_existing_wasm_without_downloading() {
        let cache_dir = tempfile::TempDir::new().unwrap();
        let parsed = parse("moonbitlang/parser/cmd/moonfmt@0.3.3");
        let mut asset = parsed.with_version("0.3.3".parse().unwrap(), "https://mooncakes.io");
        asset.cache_path = cache_dir.path().join("moonfmt.wasm");
        let wasm = b"\0asmtest";
        std::fs::write(&asset.cache_path, wasm).unwrap();

        let path = ensure_cached_wasm_with(&asset, UserDiagnostics::default(), |_| {
            bail!("cache hit should not download")
        })
        .unwrap();

        assert_eq!(path, asset.cache_path);
        assert!(!cache_dir.path().join(moonutil::common::MOON_LOCK).exists());
    }

    #[test]
    fn checksum_mismatch_rejects_download() {
        let cache_dir = tempfile::TempDir::new().unwrap();
        let parsed = parse("moonbitlang/parser/cmd/moonfmt@0.3.3");
        let mut asset = parsed.with_version("0.3.3".parse().unwrap(), "https://mooncakes.io");
        asset.cache_path = cache_dir.path().join("moonfmt.wasm");
        let expected_checksum = sha256_hex(b"expected wasm");

        let err = ensure_cached_wasm_with(&asset, UserDiagnostics::default(), |url| {
            if url.ends_with(".sha256") {
                Ok(format!("{expected_checksum}\n").into_bytes())
            } else {
                Ok(b"different wasm".to_vec())
            }
        })
        .unwrap_err()
        .to_string();

        assert!(err.contains("prebuilt wasm checksum mismatch"));
        assert!(!asset.cache_path.exists());
    }
}
