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
use tracing::debug;
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
/// The tempfile is created *inside* `install_dir` (a sibling of `dst`)
/// rather than renaming `src` straight to `dst`, because `src` lives
/// under the build tree and may sit on a different filesystem from
/// `install_dir` — think external drives, network mounts, or a
/// tmpfs-backed target dir. A cross-filesystem rename fails with
/// `EXDEV` on Unix / `ERROR_NOT_SAME_DEVICE` on Windows; copying the
/// bytes first guarantees the final rename is same-filesystem and
/// therefore atomic.
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
/// atomically replaced by `MoveFileExW(REPLACE_EXISTING)`. The fast
/// path uses `SetFileInformationByHandle(FileRenameInfoEx)` with
/// `FILE_RENAME_FLAG_POSIX_SEMANTICS`, available on Windows 10 1709+
/// — it behaves like Unix `rename(2)` and succeeds even against a
/// running target. If that syscall isn't recognized (older Windows)
/// we fall through to the legacy rename-away pattern: move the old
/// file aside, drop the new one in place, then best-effort
/// `DeleteFileW` the stray file. On contemporary Windows the
/// loader's handle usually permits shared delete, so the stray is
/// unlinked immediately and its contents are reclaimed when the
/// running target exits; if that assumption fails, the delete fails
/// and we warn rather than abort.
fn install_file(src: &Path, dst: &Path) -> anyhow::Result<()> {
    let install_dir = dst.parent().ok_or_else(|| {
        anyhow::anyhow!("Install path `{}` has no parent directory", dst.display())
    })?;

    // A `TempPath` (not `NamedTempFile`) so the file has no lingering
    // handle — on Windows we need to be able to rename over it, which
    // fails while a write handle is still open.
    let tmp_path = tempfile::NamedTempFile::new_in(install_dir)
        .with_context(|| {
            format!(
                "Failed to create temporary file in `{}`",
                install_dir.display()
            )
        })?
        .into_temp_path();

    // Try to move `src` into place without copying bytes. Works when
    // `src` and `install_dir` share a filesystem. On cross-filesystem
    // we get `EXDEV` / `ERROR_NOT_SAME_DEVICE` and fall back to a copy.
    match std::fs::rename(src, &tmp_path) {
        Ok(()) => debug!(src = %src.display(), "staged via same-fs rename"),
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            debug!(src = %src.display(), "staging via copy (cross-fs)");
            std::fs::copy(src, &tmp_path).with_context(|| {
                format!(
                    "Failed to copy binary from `{}` to `{}`",
                    src.display(),
                    tmp_path.display()
                )
            })?;
        }
        Err(e) => {
            return Err(e).with_context(|| {
                format!(
                    "Failed to stage binary from `{}` to `{}`",
                    src.display(),
                    tmp_path.display()
                )
            });
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tmp_path, perms)?;
    }

    #[cfg(windows)]
    {
        // Fast path on Windows 10 1709+: a single atomic syscall that
        // atomically replaces `dst` with `tmp_path` using POSIX rename
        // semantics — the existing `dst` inode is unlinked but kept
        // alive for any process currently executing from it, and
        // future executions resolve the name to the new file. On
        // older Windows this returns `ERROR_INVALID_PARAMETER` (the
        // info class isn't recognized); any other error just falls
        // through to the legacy persist + rename-away pattern.
        match windows::posix_replace(&tmp_path, dst) {
            Ok(()) => {
                debug!(dst = %dst.display(), "installed via FileRenameInfoEx (POSIX rename)");
                // File was moved; drop on the TempPath is a harmless no-op.
                drop(tmp_path);
                return Ok(());
            }
            Err(e) => {
                debug!(
                    err = %e,
                    code = ?e.raw_os_error(),
                    "FileRenameInfoEx rejected, falling through to persist"
                );
            }
        }
    }

    match tmp_path.persist(dst) {
        Ok(()) => {
            debug!(dst = %dst.display(), "installed via std::fs::rename (persist)");
            Ok(())
        }
        #[cfg(windows)]
        Err(err) => {
            debug!(
                err = %err.error,
                "persist failed, engaging rename-away fallback"
            );
            windows::rename_away_and_persist(err.path, dst)
        }
        #[cfg(not(windows))]
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

    /// Atomically replace `dst` with `src` using `FileRenameInfoEx` and
    /// `FILE_RENAME_FLAG_POSIX_SEMANTICS`.
    ///
    /// Available on Windows 10 1709 (Fall Creators Update) and later.
    /// Unlike `MoveFileExW(REPLACE_EXISTING)`, this succeeds even when
    /// `dst` is currently mapped by the image loader: the destination
    /// inode is unlinked-but-kept-alive for any process holding a
    /// handle, and the source's directory entry takes the destination's
    /// name. Future `CreateProcess` / `execve` calls resolve the name
    /// to the new file.
    ///
    /// On older Windows this returns `ERROR_INVALID_PARAMETER` because
    /// the `FileRenameInfoEx` info class isn't recognized; the caller
    /// falls back to the manual rename-away pattern.
    ///
    /// Windows Defender and similar real-time scanners routinely open
    /// recently-touched executables for inspection without granting
    /// `FILE_SHARE_DELETE`, causing transient `ERROR_ACCESS_DENIED`
    /// when we try to replace `dst`. We retry a few times with short
    /// backoff to ride out the scan window; if it still fails, the
    /// caller falls through to the legacy path.
    pub fn posix_replace(src: &Path, dst: &Path) -> std::io::Result<()> {
        // Short backoffs tuned for AV-scan windows (typically < 100ms).
        const RETRY_DELAYS_MS: &[u64] = &[25, 75, 200];

        let mut last_err = None;
        for (attempt, &delay_ms) in std::iter::once(&0u64)
            .chain(RETRY_DELAYS_MS.iter())
            .enumerate()
        {
            if delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            }
            match posix_replace_once(src, dst) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    // Only retry transient sharing-violation-class errors.
                    // Other errors (INVALID_PARAMETER on old Windows,
                    // DISK_FULL, etc.) won't recover from a retry.
                    let code = e.raw_os_error();
                    let transient = matches!(code, Some(5 | 32 | 33));
                    if !transient || attempt == RETRY_DELAYS_MS.len() {
                        return Err(e);
                    }
                    tracing::debug!(
                        attempt = attempt + 1,
                        code = ?code,
                        "posix_replace transient error, retrying"
                    );
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::Other, "posix_replace retry loop exited unexpectedly")
        }))
    }

    fn posix_replace_once(src: &Path, dst: &Path) -> std::io::Result<()> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileW, DELETE, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
            FileRenameInfoEx, OPEN_EXISTING, SetFileInformationByHandle,
        };

        // Some Windows builds are picky about DELETE alone on a
        // synchronous handle — request SYNCHRONIZE explicitly even
        // though it's normally implicit for non-overlapped opens.
        const SYNCHRONIZE: u32 = 0x0010_0000;
        const FILE_RENAME_FLAG_REPLACE_IF_EXISTS: u32 = 0x1;
        const FILE_RENAME_FLAG_POSIX_SEMANTICS: u32 = 0x2;

        let src_wide: Vec<u16> = src
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let handle = unsafe {
            CreateFileW(
                src_wide.as_ptr(),
                DELETE | SYNCHRONIZE,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }

        // FILE_RENAME_INFO layout (x86_64):
        //   0..4   Flags         (u32)
        //   4..8   padding       (for HANDLE alignment)
        //   8..16  RootDirectory (HANDLE, null here)
        //   16..20 FileNameLength (u32, bytes not chars)
        //   20..   FileName      (WCHAR[])
        #[cfg(target_pointer_width = "64")]
        const HEADER_SIZE: usize = 20;
        #[cfg(target_pointer_width = "32")]
        const HEADER_SIZE: usize = 12;

        let dst_wide: Vec<u16> = dst.as_os_str().encode_wide().collect();
        let name_byte_len = dst_wide.len() * 2;
        let total_size = HEADER_SIZE + name_byte_len;
        let mut buffer = vec![0u8; total_size];

        let flags = FILE_RENAME_FLAG_POSIX_SEMANTICS | FILE_RENAME_FLAG_REPLACE_IF_EXISTS;
        buffer[0..4].copy_from_slice(&flags.to_le_bytes());
        // RootDirectory stays zero (null).
        let name_len_offset = HEADER_SIZE - 4;
        buffer[name_len_offset..HEADER_SIZE]
            .copy_from_slice(&(name_byte_len as u32).to_le_bytes());
        for (i, &wc) in dst_wide.iter().enumerate() {
            buffer[HEADER_SIZE + i * 2..HEADER_SIZE + i * 2 + 2]
                .copy_from_slice(&wc.to_le_bytes());
        }

        let ok = unsafe {
            SetFileInformationByHandle(
                handle,
                FileRenameInfoEx,
                buffer.as_ptr().cast(),
                total_size as u32,
            )
        };
        let err = (ok == 0).then(std::io::Error::last_os_error);
        unsafe {
            CloseHandle(handle);
        }
        match err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    pub fn rename_away_and_persist(
        tmp: tempfile::TempPath,
        dst: &Path,
    ) -> anyhow::Result<()> {
        let backup = backup_path_for(dst);
        tracing::debug!(
            dst = %dst.display(),
            backup = %backup.display(),
            "rename-away: moving running binary aside before install"
        );

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
    /// On contemporary Windows, `DeleteFileW` (via `remove_file`)
    /// generally succeeds on the renamed-aside file even while the
    /// target is still running — the directory entry is unlinked
    /// immediately, and the contents are reclaimed when the running
    /// process releases its handle. This depends on the image loader
    /// opening executables with a share mode that permits delete,
    /// which is an undocumented implementation detail rather than an
    /// API contract. Empirical evidence: `ren running.exe other.exe`
    /// also works on Windows, and rename requires `DELETE` access,
    /// which requires `FILE_SHARE_DELETE` on every existing handle.
    /// Transactional alternatives like `DeleteFileTransactedW` are
    /// not an option — TxF is deprecated (see MS Learn: "Alternatives
    /// to using Transactional NTFS"). If the delete fails for any
    /// reason (filter drivers, older Windows, network filesystems),
    /// we warn and leave the `.old-<nonce>` file; the next install
    /// can sweep it up.
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
