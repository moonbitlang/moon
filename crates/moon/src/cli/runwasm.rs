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

use anyhow::{Context, bail};
use moonutil::{
    cli_support::AutoSyncFlags, cli_support::UniversalFlags, constants::is_moon_pkg_exist,
    registry::RegistryConfig, target::SurfaceTarget,
};
use reqwest::{StatusCode, header::USER_AGENT};
use sha2::{Digest, Sha256};
use tracing::instrument;

use super::{BuildFlags, RunSubcommand, registry_runner::ResolvedExecutablePackage};
use crate::user_diagnostics::UserDiagnostics;

/// Run a local package as WebAssembly or a prebuilt WebAssembly binary
#[derive(Debug, clap::Parser)]
#[clap(
    long_about = r#"Run a local package as WebAssembly or a prebuilt WebAssembly binary published as a Mooncakes asset.

Local package inputs are handled like `moon run --target wasm`:
  moon runwasm main
  moon runwasm ./main

Experimental moonrun policy forwarding:
  moon runwasm --experimental-policy moonrun-policy.toml main

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

    /// Experimental: pass a moonrun TOML policy file for moonbitlang/async runtime access.
    ///
    /// The policy applies to moonbitlang/async and moonrun-owned unstable FFI;
    /// WASI is not covered.
    #[clap(long = "experimental-policy", value_name = "PATH")]
    pub experimental_policy: Option<PathBuf>,

    /// The arguments provided to the wasm program
    #[clap(trailing_var_arg = true, num_args = 0.., allow_hyphen_values = true)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedRunWasmAsset {
    url: String,
    checksum_url: String,
    cache_path: PathBuf,
}

#[instrument(skip_all)]
pub(crate) fn run_runwasm(cli: &UniversalFlags, cmd: RunWasmSubcommand) -> anyhow::Result<i32> {
    if should_run_as_local_package(&cmd.package)? {
        return super::run_run(cli, runwasm_as_run_subcommand(cmd));
    }

    if cli.dry_run {
        bail!("--dry-run is not supported for Mooncakes assets in `moon runwasm`");
    }
    super::registry_runner::run(
        cmd.package,
        super::registry_runner::RegistryRunTarget::Wasm {
            experimental_policy: cmd.experimental_policy,
        },
        cmd.args,
        cli.quiet,
        cli.verbose,
    )
}

pub(super) fn cached_wasm_path(
    package: &ResolvedExecutablePackage,
    output: UserDiagnostics,
) -> anyhow::Result<PathBuf> {
    let registry_config = RegistryConfig::load();
    let asset = resolve_wasm_asset(package, &registry_config.registry);
    ensure_cached_wasm(&asset, output)
}

fn resolve_wasm_asset(package: &ResolvedExecutablePackage, registry: &str) -> ResolvedRunWasmAsset {
    let artifact_name = package.artifact_name(".wasm");
    let base = registry.trim_end_matches('/');
    let module = package.module_name.to_string();
    let url = if package.package_path.is_empty() {
        format!("{base}/assets/{module}@{}/{artifact_name}", package.version)
    } else {
        format!(
            "{base}/assets/{module}@{}/{}/{}",
            package.version, package.package_path, artifact_name
        )
    };

    let checksum_url = format!("{url}.sha256");
    ResolvedRunWasmAsset {
        url,
        checksum_url,
        cache_path: package.cache_path(".wasm"),
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
        moonrun_policy: cmd.experimental_policy,
        auto_sync_flags: AutoSyncFlags { frozen: false },
        build_only: false,
        profile: false,
    }
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
    super::registry_runner::ensure_cached_file(&asset.cache_path, output, |staged| {
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
        std::fs::write(staged, bytes)
            .with_context(|| format!("failed to write cache file {}", staged.display()))?;
        Ok(())
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(package_path: &str) -> ResolvedExecutablePackage {
        ResolvedExecutablePackage {
            module_name: "moonbitlang/parser".parse().unwrap(),
            package_path: package_path.to_string(),
            version: "0.3.3".parse().unwrap(),
        }
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
    fn build_asset_url() {
        let resolved = resolve_wasm_asset(&resolved("cmd/moonfmt"), "https://mooncakes.io/");
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
        let resolved = resolve_wasm_asset(&resolved(""), "https://mooncakes.io");
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
        let mut asset = resolve_wasm_asset(&resolved("cmd/moonfmt"), "https://mooncakes.io");
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
        let mut asset = resolve_wasm_asset(&resolved("cmd/moonfmt"), "https://mooncakes.io");
        asset.cache_path = cache_dir.path().join("moonfmt.wasm");
        let wasm = b"\0asmtest";
        std::fs::write(&asset.cache_path, wasm).unwrap();

        let path = ensure_cached_wasm_with(&asset, UserDiagnostics::default(), |_| {
            bail!("cache hit should not download")
        })
        .unwrap();

        assert_eq!(path, asset.cache_path);
        assert!(
            !cache_dir
                .path()
                .join(moonutil::constants::MOON_LOCK)
                .exists()
        );
    }

    #[test]
    fn checksum_mismatch_rejects_download() {
        let cache_dir = tempfile::TempDir::new().unwrap();
        let mut asset = resolve_wasm_asset(&resolved("cmd/moonfmt"), "https://mooncakes.io");
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
