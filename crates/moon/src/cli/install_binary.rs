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

use anyhow::{Context, bail};
use colored::Colorize;
use moonbuild_rupes_recta::{
    ResolveConfig,
    intent::UserIntent,
    model::{BuildPlanNode, BuildTarget, PackageId, TargetKind},
};
use mooncake::registry::{OnlineRegistry, Registry};
use moonutil::{
    cli::UniversalFlags,
    common::{FileLock, MOON_MOD, MOON_MOD_JSON, RunMode, TargetBackend},
    mooncakes::{ModuleName, RegistryConfig},
};
use semver::Version;
use std::path::{Path, PathBuf};

use crate::{
    cli::BuildFlags,
    rr_build::{self, BuildConfig, plan_build_from_resolved, preconfig_compile},
    user_diagnostics::UserDiagnostics,
};

/// Represents a parsed package specification from the command line.
#[derive(Debug, Clone)]
pub(super) struct PackageSpec {
    pub module_name: ModuleName,
    pub package_path: Option<String>,
    pub version: Option<Version>,
    pub is_wildcard: bool,
}

/// How to filter packages for installation.
#[derive(Debug, Clone, Copy)]
enum MatchKind {
    /// Match only the selected target.
    Exact,
    /// Match the selected target and all descendants under that target.
    Prefix,
}

/// The namespace where a package selector is interpreted.
#[derive(Debug, Clone)]
enum FilterTarget {
    /// Physical package root path on disk (local/git modes).
    FileSystem(PathBuf),
    /// Logical package path relative to module source root (registry mode).
    PackagePath(String),
}

/// Package selection rule used by binary installation.
///
/// The filter is intentionally split into target namespace (`FilterTarget`)
/// and matching strategy (`MatchKind`) so callers can compose selectors
/// explicitly instead of relying on ad-hoc enum variants.
#[derive(Debug, Clone)]
struct PackageFilter {
    target: FilterTarget,
    kind: MatchKind,
}

impl PackageFilter {
    /// Build a filesystem-based filter.
    fn filesystem(path: PathBuf, install_all: bool) -> Self {
        Self {
            target: FilterTarget::FileSystem(path),
            kind: if install_all {
                MatchKind::Prefix
            } else {
                MatchKind::Exact
            },
        }
    }

    /// Build a logical package-path filter.
    fn package_path(path: String, install_all: bool) -> Self {
        Self {
            target: FilterTarget::PackagePath(path),
            kind: if install_all {
                MatchKind::Prefix
            } else {
                MatchKind::Exact
            },
        }
    }

    /// Returns true if a discovered package matches this filter.
    fn matches(&self, pkg_root_path: &Path, pkg_path_str: &str) -> bool {
        match (&self.target, self.kind) {
            (FilterTarget::FileSystem(path), MatchKind::Exact) => pkg_root_path == *path,
            (FilterTarget::FileSystem(prefix_path), MatchKind::Prefix) => {
                pkg_root_path.starts_with(prefix_path)
            }
            (FilterTarget::PackagePath(target), MatchKind::Exact) => pkg_path_str == *target,
            (FilterTarget::PackagePath(prefix), MatchKind::Prefix) => {
                prefix.is_empty()
                    || pkg_path_str.starts_with(&format!("{}/", prefix))
                    || pkg_path_str == *prefix
            }
        }
    }

    /// Build a mode-appropriate "no package selected" error.
    fn no_match_error(&self, module_name: &ModuleName, module_dir: &Path) -> anyhow::Error {
        match (&self.target, self.kind) {
            (FilterTarget::FileSystem(path), MatchKind::Exact) => anyhow::anyhow!(
                "Path `{}` is not a main package (is-main: true required)",
                path.display()
            ),
            (FilterTarget::FileSystem(prefix_path), MatchKind::Prefix) => {
                if prefix_path == module_dir {
                    anyhow::anyhow!("No main packages found in module `{}`", module_name)
                } else {
                    anyhow::anyhow!(
                        "No main packages found under path `{}`",
                        prefix_path.display()
                    )
                }
            }
            (FilterTarget::PackagePath(prefix), MatchKind::Prefix) => {
                if prefix.is_empty() {
                    anyhow::anyhow!("No main packages found in module `{}`", module_name)
                } else {
                    anyhow::anyhow!(
                        "No main packages found matching pattern `{}/{}/...`",
                        module_name,
                        prefix
                    )
                }
            }
            (FilterTarget::PackagePath(target), MatchKind::Exact) => {
                let full_name = if target.is_empty() {
                    module_name.to_string()
                } else {
                    format!("{}/{}", module_name, target)
                };
                anyhow::anyhow!(
                    "Package `{}` not found or is not a main package (is-main: true required)",
                    full_name
                )
            }
        }
    }
}

const GIT_URL_PREFIXES: &[&str] = &["https://", "http://", "git://", "ssh://", "git@"];

/// Returns the non-wildcard prefix for inputs ending with `/...` or `...`.
pub(super) fn strip_wildcard_suffix(s: &str) -> Option<&str> {
    s.strip_suffix("...").map(|base| base.trim_end_matches('/'))
}

/// Check if a string looks like a git URL.
pub(super) fn is_git_url(s: &str) -> bool {
    GIT_URL_PREFIXES.iter().any(|p| s.starts_with(p))
}

/// Check if a string looks like a local filesystem path.
/// Matches: ./, ../, / (Unix absolute), C: (Windows drive letter)
pub(super) fn is_local_path(s: &str) -> bool {
    s.starts_with("./")
        || s.starts_with("../")
        || s.starts_with('/')
        || s.chars().nth(1) == Some(':') // Windows drive letter
}

/// Yet another package path parser because we need to parse wildcard patterns.
pub(super) fn parse_package_spec(input: &str) -> anyhow::Result<PackageSpec> {
    let (path_part, version) = if let Some(at_pos) = input.rfind('@') {
        let path = &input[..at_pos];
        let version_str = &input[at_pos + 1..];
        let version = Version::parse(version_str)
            .with_context(|| format!("Invalid version `{}`", version_str))?;
        (path, Some(version))
    } else {
        (input, None)
    };

    let (path_part, is_wildcard) = if let Some(stripped) = strip_wildcard_suffix(path_part) {
        (stripped, true)
    } else {
        (path_part, false)
    };

    let components: Vec<&str> = path_part.split('/').collect();

    if components.len() < 2 {
        bail!(
            "Invalid package path `{}`: must be in format `user/module/package`",
            input
        );
    }

    let module_name = ModuleName {
        username: components[0].into(),
        unqual: components[1].into(),
    };

    let package_path = if components.len() > 2 {
        Some(components[2..].join("/"))
    } else {
        // user/module or user/module/... -> package at root
        Some(String::new())
    };

    Ok(PackageSpec {
        module_name,
        package_path,
        version,
        is_wildcard,
    })
}

/// Install a binary package from the registry.
pub(super) fn install_binary(
    cli: &UniversalFlags,
    spec: &PackageSpec,
    install_dir: &Path,
    install_all: bool,
) -> anyhow::Result<i32> {
    let quiet = cli.quiet;
    let output = UserDiagnostics::from_flags(cli);

    let index_dir = moonutil::moon_dir::index();
    let registry_config = RegistryConfig::load();
    let had_index = index_dir.exists();

    match mooncake::update::update(&index_dir, &registry_config) {
        Ok(_) => {
            output.info("Updated registry index");
        }
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
    let version = if let Some(v) = &spec.version {
        v.clone()
    } else {
        let module_info = registry
            .get_latest_version(&spec.module_name)
            .ok_or_else(|| {
                anyhow::anyhow!("Module `{}` not found in registry", spec.module_name)
            })?;
        module_info.version.clone().ok_or_else(|| {
            anyhow::anyhow!("Module `{}` has no version information", spec.module_name)
        })?
    };

    output.info(format!("Installing {}@{}", spec.module_name, version));

    let tmp_dir = tempfile::TempDir::new().context("Failed to create temporary directory")?;
    let module_dir = tmp_dir.path();

    registry.install_to(&spec.module_name, &version, module_dir, quiet)?;

    let filter =
        PackageFilter::package_path(spec.package_path.clone().unwrap_or_default(), install_all);

    build_and_install_packages(cli, &spec.module_name, module_dir, install_dir, filter)
}

/// Install from a local path.
pub(super) fn install_from_local(
    cli: &UniversalFlags,
    local_path: &Path,
    install_dir: &Path,
    install_all: bool,
) -> anyhow::Result<i32> {
    let input_path = dunce::canonicalize(local_path).with_context(|| {
        format!(
            "Path `{}` does not exist or cannot be resolved",
            local_path.display()
        )
    })?;

    let module_root = moonutil::dirs::find_ancestor_with_mod(&input_path).ok_or_else(|| {
        anyhow::anyhow!(
            "Path `{}` is not in a MoonBit module (no {} or {} found in ancestors)",
            input_path.display(),
            MOON_MOD,
            MOON_MOD_JSON
        )
    })?;

    let module = moonutil::common::read_module_desc_file_in_dir(&module_root)?;
    let module_name: ModuleName = module.name.parse().map_err(|e| anyhow::anyhow!("{}", e))?;
    let filter = PackageFilter::filesystem(input_path, install_all);

    build_and_install_packages(cli, &module_name, &module_root, install_dir, filter)
}

/// Git reference type for checkout.
pub(super) enum GitRef<'a> {
    /// Checkout a specific revision (commit hash)
    Rev(&'a str),
    /// Checkout a branch
    Branch(&'a str),
    /// Checkout a tag
    Tag(&'a str),
    /// Use default branch
    Default,
}

/// Install from a git repository.
pub(super) fn install_from_git(
    cli: &UniversalFlags,
    git_url: &str,
    git_ref: GitRef<'_>,
    path_in_repo: Option<&str>,
    install_dir: &Path,
    install_all: bool,
) -> anyhow::Result<i32> {
    let output = UserDiagnostics::from_flags(cli);

    output.info(format!("Cloning `{}`...", git_url));

    let tmp_dir = tempfile::TempDir::new().context("Failed to create temporary directory")?;
    let clone_dir = tmp_dir.path();

    // Clone the repository
    let mut clone_cmd = std::process::Command::new(moonutil::BINARIES.git_or_default());
    clone_cmd.arg("clone");

    match git_ref {
        GitRef::Branch(branch) => {
            clone_cmd.args(["--depth", "1", "--branch", branch]);
        }
        GitRef::Tag(tag) => {
            clone_cmd.args(["--depth", "1", "--branch", tag]);
        }
        GitRef::Default => {
            clone_cmd.args(["--depth", "1"]);
        }
        GitRef::Rev(_) => {
            // For rev, need full clone (specific commit may not be in shallow history)
        }
    }

    clone_cmd.arg(git_url).arg(clone_dir);

    let status = clone_cmd.status().context("Failed to execute git clone")?;

    if !status.success() {
        bail!("Failed to clone repository `{}`", git_url);
    }

    // If rev specified, checkout to that commit
    if let GitRef::Rev(rev) = git_ref {
        let status = std::process::Command::new(moonutil::BINARIES.git_or_default())
            .current_dir(clone_dir)
            .args(["checkout", rev])
            .status()
            .context("Failed to checkout revision")?;

        if !status.success() {
            bail!("Failed to checkout revision `{}`", rev);
        }
    }

    // Determine the target path within the cloned repo
    let target_path = if let Some(repo_path) = path_in_repo {
        let repo_path = repo_path.trim_matches('/');
        let repo_path = repo_path.trim_end_matches("/...").trim_end_matches("...");
        if repo_path.is_empty() {
            clone_dir.to_path_buf()
        } else {
            clone_dir.join(repo_path)
        }
    } else {
        clone_dir.to_path_buf()
    };

    // Check if target path exists
    if !target_path.exists() {
        bail!(
            "Path `{}` does not exist in the repository",
            path_in_repo.unwrap_or("")
        );
    }

    let target_path = dunce::canonicalize(&target_path).with_context(|| {
        format!(
            "Path `{}` cannot be resolved in the repository",
            path_in_repo.unwrap_or("")
        )
    })?;
    let clone_dir =
        dunce::canonicalize(clone_dir).context("Failed to resolve cloned repository")?;
    if !target_path.starts_with(&clone_dir) {
        bail!(
            "Path `{}` escapes repository root",
            path_in_repo.unwrap_or("")
        );
    }

    // Find module root
    let module_root = moonutil::dirs::find_ancestor_with_mod(&target_path).ok_or_else(|| {
        anyhow::anyhow!("No {} or {} found in repository", MOON_MOD, MOON_MOD_JSON)
    })?;

    let module = moonutil::common::read_module_desc_file_in_dir(&module_root)?;
    let module_name: ModuleName = module.name.parse().map_err(|e| anyhow::anyhow!("{}", e))?;
    let filter = PackageFilter::filesystem(target_path, install_all);

    build_and_install_packages(cli, &module_name, &module_root, install_dir, filter)
}

/// Build matching packages and install binaries using RR build engine.
fn build_and_install_packages(
    cli: &UniversalFlags,
    module_name: &ModuleName,
    module_dir: &Path,
    install_dir: &Path,
    filter: PackageFilter,
) -> anyhow::Result<i32> {
    let quiet = cli.quiet;
    let output = UserDiagnostics::from_flags(cli);
    let package_dirs = cli
        .source_tgt_dir
        .package_dirs_from_source_root(module_dir)?;
    let source_dir = package_dirs.source_dir;
    let target_dir = package_dirs.target_dir;
    let mooncakes_dir = package_dirs.mooncakes_dir;

    let resolve_cfg = ResolveConfig::new_with_load_defaults(false, false, false);
    let resolve_output = moonbuild_rupes_recta::resolve(&resolve_cfg, &source_dir, &mooncakes_dir)?;

    let main_module_id = resolve_output.local_modules()[0];
    let Some(all_pkgs) = resolve_output.pkg_dirs.packages_for_module(main_module_id) else {
        bail!(
            "No packages found in module at path `{}`",
            module_dir.display()
        );
    };

    struct SelectedPackage {
        pkg_id: PackageId,
        full_pkg_name: String,
        binary_name: String,
    }

    let mut selected_packages: Vec<SelectedPackage> = Vec::new();

    for (pkg_path, &pkg_id) in all_pkgs {
        let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
        if !pkg.raw.is_main {
            continue;
        }

        let pkg_path_str = pkg_path.to_string();

        let matched = filter.matches(&pkg.root_path, &pkg_path_str);

        if matched {
            let binary_name = pkg_path_str
                .rsplit('/')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or(&module_name.unqual)
                .to_string();
            let full_pkg_name = if pkg_path_str.is_empty() {
                module_name.to_string()
            } else {
                format!("{}/{}", module_name, pkg_path_str)
            };
            selected_packages.push(SelectedPackage {
                pkg_id,
                full_pkg_name,
                binary_name,
            });
        }
    }

    if selected_packages.is_empty() {
        return Err(filter.no_match_error(module_name, module_dir));
    }

    if cli.dry_run {
        let mut dry_run_count = 0;
        for pkg in &selected_packages {
            if moonutil::moon_dir::RESERVED_BIN_NAMES.contains(&pkg.binary_name.as_str()) {
                output.error(format!(
                    "Cannot install `{}` - name conflicts with MoonBit toolchain binary",
                    pkg.binary_name
                ));
                continue;
            }
            let dst_name = if cfg!(windows) {
                format!("{}.exe", pkg.binary_name)
            } else {
                pkg.binary_name.clone()
            };
            let binary_dst = install_dir.join(dst_name);
            eprintln!("{}: Would build `{}`", "Dry-run".cyan(), pkg.full_pkg_name);
            eprintln!(
                "{}: Would install `{}` to `{}`",
                "Dry-run".cyan(),
                pkg.binary_name,
                binary_dst.display()
            );
            dry_run_count += 1;
        }
        if dry_run_count == 0 {
            bail!("No packages would be installed");
        }
        return Ok(0);
    }

    std::fs::create_dir_all(install_dir).with_context(|| {
        format!(
            "Failed to create install directory `{}`",
            install_dir.display()
        )
    })?;

    std::fs::create_dir_all(&target_dir).context("Failed to create build directory")?;
    let mut installed_count = 0;

    for pkg in selected_packages {
        // Check if binary name would overwrite a reserved toolchain binary
        if moonutil::moon_dir::RESERVED_BIN_NAMES.contains(&pkg.binary_name.as_str()) {
            output.error(format!(
                "Cannot install `{}` - name conflicts with MoonBit toolchain binary",
                pkg.binary_name
            ));
            continue;
        }

        output.info(format!("Building `{}`...", pkg.full_pkg_name));

        let build_flags = BuildFlags {
            release: true,
            warn_list: Some("-a".to_string()),
            ..BuildFlags::default()
        };
        let preconfig = preconfig_compile(
            &moonutil::mooncakes::sync::AutoSyncFlags { frozen: false },
            cli,
            &build_flags,
            Some(TargetBackend::Native),
            &target_dir,
            RunMode::Build,
        );

        let (build_meta, build_graph) = plan_build_from_resolved(
            preconfig,
            &cli.unstable_feature,
            &target_dir,
            UserDiagnostics::from_flags(cli),
            Box::new(move |_, _| Ok(vec![UserIntent::Build(pkg.pkg_id)].into())),
            resolve_output.clone(),
        )?;

        let _lock = FileLock::lock(&target_dir)?;
        rr_build::generate_all_pkgs_json(&target_dir, &build_meta, RunMode::Build)?;

        let result = rr_build::execute_build(
            &BuildConfig::from_flags(
                &build_flags,
                &cli.unstable_feature,
                cli.verbose,
                UserDiagnostics::from_flags(cli),
            ),
            build_graph,
            &target_dir,
        )?;
        if !result.successful() {
            result.print_info(quiet, "building").ok();
            output.error(format!("Failed to build `{}`", pkg.full_pkg_name));
            continue;
        }
        result.print_info(quiet, "building").ok();

        let target = BuildTarget {
            package: pkg.pkg_id,
            kind: TargetKind::Source,
        };
        let binary_src =
            build_meta.artifacts[&BuildPlanNode::MakeExecutable(target)].artifacts[0].clone();
        let dst_name = if cfg!(windows) {
            format!("{}.exe", pkg.binary_name)
        } else {
            pkg.binary_name.clone()
        };
        let binary_dst = install_dir.join(dst_name);

        install_file(&binary_src, &binary_dst)?;

        if !quiet {
            eprintln!(
                "{}: Installed `{}` to `{}`",
                "Success".green().bold(),
                pkg.binary_name,
                binary_dst.display()
            );
        }

        installed_count += 1;
    }

    if installed_count == 0 {
        bail!("No packages were successfully installed");
    }

    Ok(0)
}

/// Copy `src` onto `dst` by writing to a sibling tempfile and atomically
/// renaming it into place.
///
/// Overwriting the destination with `fs::copy` would truncate it via
/// `O_TRUNC`; on macOS the kernel SIGKILLs any process that re-execs the
/// modified inode because the code-signature cache no longer matches the
/// on-disk bytes. On Linux, O_TRUNC on a running text segment returns
/// `ETXTBSY` and the install fails outright. Renaming a fresh file into
/// place avoids both — the old inode is unlinked but remains alive for
/// the running process, and new executions resolve to the new inode.
///
/// On Windows the file is held by the image loader and cannot be
/// overwritten; we fall back to the rename-away pattern (move the old
/// file aside, drop the new one in place, then `DeleteFileW` the
/// stray file — the loader grants `FILE_SHARE_DELETE`, so the
/// directory entry is unlinked immediately and its contents are
/// reclaimed when the running target exits).
fn install_file(src: &Path, dst: &Path) -> anyhow::Result<()> {
    let install_dir = dst.parent().ok_or_else(|| {
        anyhow::anyhow!("Install path `{}` has no parent directory", dst.display())
    })?;

    let tmp = tempfile::NamedTempFile::new_in(install_dir).with_context(|| {
        format!(
            "Failed to create temporary file in `{}`",
            install_dir.display()
        )
    })?;

    std::fs::copy(src, tmp.path()).with_context(|| {
        format!(
            "Failed to copy binary from `{}` to `{}`",
            src.display(),
            tmp.path().display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(tmp.path())?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(tmp.path(), perms)?;
    }

    match tmp.persist(dst) {
        Ok(_) => Ok(()),
        #[cfg(windows)]
        Err(err) if windows::is_sharing_violation(&err.error) => {
            windows::rename_away_and_persist(err.file, dst)
        }
        Err(err) => Err(err.error).with_context(|| {
            format!("Failed to install binary to `{}`", dst.display())
        }),
    }
}

#[cfg(windows)]
mod windows {
    use std::path::{Path, PathBuf};

    use anyhow::Context;
    use colored::Colorize;

    /// Windows error codes that indicate the destination is locked by a
    /// running executable (or otherwise non-deletable).
    ///
    /// - 5  = ERROR_ACCESS_DENIED (typical for a running `.exe`)
    /// - 32 = ERROR_SHARING_VIOLATION
    /// - 33 = ERROR_LOCK_VIOLATION
    pub fn is_sharing_violation(error: &std::io::Error) -> bool {
        matches!(error.raw_os_error(), Some(5 | 32 | 33))
    }

    pub fn rename_away_and_persist(
        tmp: tempfile::NamedTempFile,
        dst: &Path,
    ) -> anyhow::Result<()> {
        let backup = backup_path_for(dst);

        std::fs::rename(dst, &backup).with_context(|| {
            format!(
                "Failed to rename running binary `{}` aside",
                dst.display()
            )
        })?;

        if let Err(err) = tmp.persist(dst) {
            // Second rename failed for an unexpected reason — put the
            // original back so the user isn't left without a binary.
            let _ = std::fs::rename(&backup, dst);
            return Err(err.error).with_context(|| {
                format!("Failed to place new binary at `{}`", dst.display())
            });
        }

        cleanup_backup(&backup);

        Ok(())
    }

    /// Best-effort cleanup of the renamed-aside file.
    ///
    /// `DeleteFileW` (via `remove_file`) succeeds even while the target
    /// is still running, because the image loader opens executables
    /// with `FILE_SHARE_DELETE`. The directory entry is unlinked
    /// immediately and the contents are reclaimed when the running
    /// target exits. Failure is non-fatal — we just leave the file on
    /// disk with a warning; the next install will sweep it up.
    fn cleanup_backup(backup: &Path) {
        if let Err(e) = std::fs::remove_file(backup) {
            eprintln!(
                "{}: could not remove leftover `{}`: {}",
                "Warning".yellow().bold(),
                backup.display(),
                e
            );
        }
    }

    fn backup_path_for(dst: &Path) -> PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut s = dst.as_os_str().to_owned();
        s.push(format!(".old-{nonce}"));
        PathBuf::from(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path(parts: &[&str]) -> PathBuf {
        let mut path = PathBuf::new();
        for part in parts {
            path.push(part);
        }
        path
    }

    #[test]
    fn test_is_git_url() {
        // Valid git URLs
        assert!(is_git_url("https://github.com/user/repo"));
        assert!(is_git_url("https://gitlab.com/user/repo.git"));
        assert!(is_git_url("http://github.com/user/repo"));
        assert!(is_git_url("git://github.com/user/repo"));
        assert!(is_git_url("ssh://git@github.com/user/repo"));
        assert!(is_git_url("git@github.com:user/repo.git"));
        assert!(is_git_url("git@gitlab.com:group/subgroup/repo.git"));

        // Not git URLs (registry paths)
        assert!(!is_git_url("user/repo"));
        assert!(!is_git_url("user/repo/cmd/main"));
        assert!(!is_git_url("Lampese/moonbead"));

        // Not git URLs (local paths)
        assert!(!is_git_url("./local/path"));
        assert!(!is_git_url("/absolute/path"));
    }

    #[test]
    fn test_is_local_path() {
        // Relative paths
        assert!(is_local_path("./local/path"));
        assert!(is_local_path("./"));
        assert!(is_local_path("../parent/path"));
        assert!(is_local_path("../"));

        // Unix absolute paths
        assert!(is_local_path("/absolute/path"));
        assert!(is_local_path("/"));

        // Windows drive letters
        assert!(is_local_path("C:\\path\\to\\dir"));
        assert!(is_local_path("C:/path/to/dir"));
        assert!(is_local_path("D:\\"));
        assert!(is_local_path("D:/"));

        // Not local paths (registry paths)
        assert!(!is_local_path("user/repo"));
        assert!(!is_local_path("user/repo/cmd/main"));
        assert!(!is_local_path("Lampese/moonbead"));

        // Not local paths (git URLs)
        assert!(!is_local_path("https://github.com/user/repo"));
        assert!(!is_local_path("git@github.com:user/repo.git"));
    }

    #[test]
    fn test_parse_package_spec_basic() {
        // Basic user/module
        let spec = parse_package_spec("user/module").unwrap();
        assert_eq!(spec.module_name.username, "user");
        assert_eq!(spec.module_name.unqual, "module");
        assert_eq!(spec.package_path, Some(String::new()));
        assert_eq!(spec.version, None);
        assert!(!spec.is_wildcard);

        // user/module/package
        let spec = parse_package_spec("user/module/cmd/main").unwrap();
        assert_eq!(spec.module_name.username, "user");
        assert_eq!(spec.module_name.unqual, "module");
        assert_eq!(spec.package_path, Some("cmd/main".to_string()));
        assert!(!spec.is_wildcard);
    }

    #[test]
    fn test_parse_package_spec_with_version() {
        let spec = parse_package_spec("user/module@1.0.0").unwrap();
        assert_eq!(spec.module_name.username, "user");
        assert_eq!(spec.module_name.unqual, "module");
        assert_eq!(spec.version.unwrap().to_string(), "1.0.0");
        assert!(!spec.is_wildcard);

        let spec = parse_package_spec("user/module/cmd/main@2.3.4").unwrap();
        assert_eq!(spec.package_path, Some("cmd/main".to_string()));
        assert_eq!(spec.version.unwrap().to_string(), "2.3.4");
    }

    #[test]
    fn test_parse_package_spec_wildcard() {
        // user/module/...
        let spec = parse_package_spec("user/module/...").unwrap();
        assert_eq!(spec.module_name.username, "user");
        assert_eq!(spec.module_name.unqual, "module");
        assert!(spec.is_wildcard);
        assert_eq!(spec.package_path, Some(String::new()));

        // user/module/cmd/...
        let spec = parse_package_spec("user/module/cmd/...").unwrap();
        assert!(spec.is_wildcard);
        assert_eq!(spec.package_path, Some("cmd".to_string()));

        // Alternate syntax: user/module...
        let spec = parse_package_spec("user/module...").unwrap();
        assert!(spec.is_wildcard);
    }

    #[test]
    fn test_parse_package_spec_invalid() {
        // Too few components
        assert!(parse_package_spec("user").is_err());
        assert!(parse_package_spec("single").is_err());

        // Invalid version
        assert!(parse_package_spec("user/module@invalid").is_err());
    }

    #[test]
    fn test_package_filter_matches_by_path() {
        let path = test_path(&["repo", "examples", "native", "pixeladventure"]);
        let filter = PackageFilter::filesystem(path.clone(), false);

        assert!(filter.matches(&path, "native/pixeladventure"));
        assert!(!filter.matches(
            &test_path(&["repo", "examples", "native", "cards"]),
            "native/cards"
        ));
    }

    #[test]
    fn test_package_filter_matches_by_path_prefix() {
        let filter = PackageFilter::filesystem(test_path(&["repo", "examples", "native"]), true);

        assert!(filter.matches(
            &test_path(&["repo", "examples", "native", "pixeladventure"]),
            "native/pixeladventure",
        ));
        assert!(filter.matches(
            &test_path(&["repo", "examples", "native", "cards"]),
            "native/cards",
        ));
        assert!(!filter.matches(&test_path(&["repo", "examples", "web", "demo"]), "web/demo"));
    }

    #[test]
    fn test_package_filter_matches_by_package_prefix() {
        let filter = PackageFilter::package_path("native".to_string(), true);
        assert!(filter.matches(
            &test_path(&["repo", "examples", "native", "pixeladventure"]),
            "native/pixeladventure",
        ));
        assert!(!filter.matches(&test_path(&["repo", "examples", "web", "demo"]), "web/demo",));
    }
}
