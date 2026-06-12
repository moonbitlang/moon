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
use moonbuild_rupes_recta::{ResolveConfig, model::PackageId};
use mooncake::registry::{OnlineRegistry, Registry, path as registry_path};
use moonutil::{
    cli::UniversalFlags,
    common::{MOON_MOD, MOON_MOD_JSON, MOON_WORK_ENV},
    dirs::{PackageDirs, WorkspaceEnv},
    mooncakes::{ModuleId, ModuleName, ModuleSourceKind, RegistryConfig},
};
use semver::Version;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::{rr_build, user_diagnostics::UserDiagnostics};

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

const GIT_URL_PREFIXES: &[&str] = &["https://", "http://", "git://", "ssh://", "git@", "file://"];

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

pub(super) fn parse_package_spec(input: &str) -> anyhow::Result<PackageSpec> {
    let (path_part, version) = if let Some(at_pos) = input.rfind('@') {
        let path = &input[..at_pos];
        let version_str = &input[at_pos + 1..];
        (path, Some(version_str))
    } else {
        (input, None)
    };

    let (path_part, is_wildcard) = if let Some(stripped) = strip_wildcard_suffix(path_part) {
        (stripped, true)
    } else {
        (path_part, false)
    };

    let (parsed, version) = if let Some(version) = version {
        let normalized = format!("{path_part}@{version}");
        let parsed = registry_path::parse_package_at_version_path(&normalized)
            .with_context(|| format!("Invalid package path `{input}`"))?;
        let version = Version::parse(&parsed.version)
            .with_context(|| format!("Invalid version `{}`", parsed.version))?;
        (
            registry_path::InstallStylePath {
                module: parsed.module,
                package: parsed.package,
            },
            Some(version),
        )
    } else {
        let parsed = registry_path::parse_install_style_path(path_part)
            .with_context(|| format!("Invalid package path `{input}`"))?;
        (parsed, None)
    };

    Ok(PackageSpec {
        module_name: parsed.module,
        package_path: Some(parsed.package),
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
    let package_dirs = cli.source_tgt_dir.source_root_package_dirs(module_dir)?;

    build_and_install_packages(
        cli,
        InstallSourceProject {
            module_name: spec.module_name.clone(),
            module_root: module_dir.to_path_buf(),
            build_cwd: module_dir.to_path_buf(),
            package_dirs,
            workspace_env: WorkspaceEnv::Off,
        },
        install_dir,
        filter,
    )
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

    let project = source_install_project(cli, &input_path).with_context(|| {
        format!(
            "Path `{}` is not in a MoonBit module (no {} or {} found in ancestors)",
            input_path.display(),
            MOON_MOD,
            MOON_MOD_JSON
        )
    })?;
    let filter = PackageFilter::filesystem(input_path, install_all);

    build_and_install_packages(cli, project, install_dir, filter)
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

struct SelectedPackage {
    pkg_id: PackageId,
    root_path: PathBuf,
    full_pkg_name: String,
    binary_name: String,
}

struct InstallSourceProject {
    module_name: ModuleName,
    module_root: PathBuf,
    build_cwd: PathBuf,
    package_dirs: PackageDirs,
    workspace_env: WorkspaceEnv,
}

fn source_install_project(
    cli: &UniversalFlags,
    source_path: &Path,
) -> anyhow::Result<InstallSourceProject> {
    let source_dirs = moonutil::dirs::SourceTargetDirs {
        cwd: None,
        manifest_path: None,
        target_dir: cli.source_tgt_dir.target_dir.clone(),
    };
    let mut query = source_dirs.query_from(source_path, WorkspaceEnv::Auto)?;
    let project = query.project()?;
    let selected_module = project.selected_module().ok_or_else(|| {
        anyhow::anyhow!(
            "Path `{}` is in a workspace but does not select a MoonBit module",
            source_path.display()
        )
    })?;
    let workspace_env = project
        .workspace_ref()
        .map(|workspace| WorkspaceEnv::Pinned(workspace.manifest_path))
        .unwrap_or(WorkspaceEnv::Off);
    let module = moonutil::common::read_module_desc_file_in_dir(&selected_module.root)?;
    let module_name: ModuleName = module.name.parse().map_err(|e| anyhow::anyhow!("{}", e))?;
    let package_dirs = query.package_dirs()?;
    let build_cwd = package_dirs.source_dir.clone();

    Ok(InstallSourceProject {
        module_name,
        module_root: selected_module.root,
        build_cwd,
        package_dirs,
        workspace_env,
    })
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

    let mut project = source_install_project(cli, &target_path)
        .with_context(|| format!("No {} or {} found in repository", MOON_MOD, MOON_MOD_JSON))?;
    let filter = PackageFilter::filesystem(target_path, install_all);
    project.build_cwd = clone_dir;

    build_and_install_packages(cli, project, install_dir, filter)
}

fn module_manifest_path(module_dir: &Path) -> anyhow::Result<PathBuf> {
    let moon_mod = module_dir.join(MOON_MOD);
    if moon_mod.exists() {
        return Ok(moon_mod);
    }
    let moon_mod_json = module_dir.join(MOON_MOD_JSON);
    if moon_mod_json.exists() {
        return Ok(moon_mod_json);
    }
    bail!(
        "No {} or {} found in module root `{}`",
        MOON_MOD,
        MOON_MOD_JSON,
        module_dir.display()
    )
}

fn run_build_for_install(
    cli: &UniversalFlags,
    build_cwd: &Path,
    module_dir: &Path,
    project_manifest_path: Option<&Path>,
    target_dir: &Path,
    workspace_env: &WorkspaceEnv,
    packages: &[SelectedPackage],
) -> anyhow::Result<i32> {
    let current_moon = std::env::current_exe().context("Failed to resolve current moon binary")?;
    let mut cmd = Command::new(&current_moon);
    cmd.env("MOON_OVERRIDE", current_moon);
    match workspace_env {
        WorkspaceEnv::Auto => {}
        WorkspaceEnv::Off => {
            cmd.env(MOON_WORK_ENV, "off");
        }
        WorkspaceEnv::Pinned(workspace_path) => {
            cmd.env(MOON_WORK_ENV, workspace_path);
        }
    }
    cmd.arg("-C").arg(build_cwd);
    if let Some(project_manifest_path) = project_manifest_path {
        cmd.arg("--manifest-path").arg(project_manifest_path);
    } else if build_cwd != module_dir {
        cmd.arg("--manifest-path")
            .arg(module_manifest_path(module_dir)?);
    }
    cmd.arg("--target-dir").arg(target_dir);

    if cli.quiet {
        cmd.arg("--quiet");
    }
    if cli.verbose {
        cmd.arg("--verbose");
    }
    if cli.trace {
        cmd.arg("--trace");
    }
    if cli.build_graph {
        cmd.arg("--build-graph");
    }
    let unstable_features = cli.unstable_feature.to_string();
    if !unstable_features.is_empty() {
        cmd.arg("-Z").arg(unstable_features);
    }

    cmd.args(["build", "--release", "--target", "native", "--warn-list=-a"]);
    cmd.args(packages.iter().map(|pkg| &pkg.root_path));

    let status = cmd
        .status()
        .context("Failed to spawn binary install build")?;
    Ok(status.code().unwrap_or(1))
}

fn selected_module_id(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    module_dir: &Path,
) -> Option<ModuleId> {
    resolve_output
        .local_modules()
        .iter()
        .copied()
        .find(|&module_id| {
            matches!(
                resolve_output.module_rel.module_source(module_id).source(),
                ModuleSourceKind::Local(path) if path == module_dir
            )
        })
}

/// Resolve matching packages, build them with `moon build`, and install binaries.
fn build_and_install_packages(
    cli: &UniversalFlags,
    source: InstallSourceProject,
    install_dir: &Path,
    filter: PackageFilter,
) -> anyhow::Result<i32> {
    let quiet = cli.quiet;
    let output = UserDiagnostics::from_flags(cli);
    let source_dir = source.package_dirs.source_dir;
    let target_dir = source.package_dirs.target_dir;
    let mooncakes_dir = source.package_dirs.mooncakes_dir;
    let project_manifest_path = source.package_dirs.project_manifest_path;

    let build_workspace_env = source.workspace_env.clone();
    let resolve_cfg =
        ResolveConfig::new_with_load_defaults(false, false, false, source.workspace_env);
    let synced_env = moonbuild_rupes_recta::sync_dependencies(
        &resolve_cfg,
        &source_dir,
        &mooncakes_dir,
        project_manifest_path.as_deref(),
    )?;
    let resolve_output = moonbuild_rupes_recta::resolve_synced_project(&resolve_cfg, synced_env)?;

    let Some(main_module_id) = selected_module_id(&resolve_output, &source.module_root) else {
        bail!(
            "Selected module `{}` was not found in resolved project",
            source.module_root.display()
        );
    };
    let Some(all_pkgs) = resolve_output.pkg_dirs.packages_for_module(main_module_id) else {
        bail!(
            "No packages found in module at path `{}`",
            source.module_root.display()
        );
    };

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
                .unwrap_or(&source.module_name.unqual)
                .to_string();
            let full_pkg_name = if pkg_path_str.is_empty() {
                source.module_name.to_string()
            } else {
                format!("{}/{}", source.module_name, pkg_path_str)
            };
            selected_packages.push(SelectedPackage {
                pkg_id,
                root_path: pkg.root_path.clone(),
                full_pkg_name,
                binary_name,
            });
        }
    }

    if selected_packages.is_empty() {
        return Err(filter.no_match_error(&source.module_name, &source.module_root));
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
    let mut installable_packages: Vec<SelectedPackage> = Vec::new();

    for pkg in selected_packages {
        // Check if binary name would overwrite a reserved toolchain binary
        if moonutil::moon_dir::RESERVED_BIN_NAMES.contains(&pkg.binary_name.as_str()) {
            output.error(format!(
                "Cannot install `{}` - name conflicts with MoonBit toolchain binary",
                pkg.binary_name
            ));
            continue;
        }
        installable_packages.push(pkg);
    }

    if installable_packages.is_empty() {
        bail!("No packages were successfully installed");
    }

    for pkg in &installable_packages {
        output.info(format!("Building `{}`...", pkg.full_pkg_name));
    }

    let build_status = run_build_for_install(
        cli,
        &source.build_cwd,
        &source.module_root,
        project_manifest_path.as_deref(),
        &target_dir,
        &build_workspace_env,
        &installable_packages,
    )?;
    if build_status != 0 {
        return Ok(build_status);
    }

    for pkg in installable_packages {
        let binary_src =
            rr_build::native_source_executable_path(&target_dir, &resolve_output, pkg.pkg_id);
        let dst_name = if cfg!(windows) {
            format!("{}.exe", pkg.binary_name)
        } else {
            pkg.binary_name.clone()
        };
        let binary_dst = install_dir.join(dst_name);

        std::fs::copy(&binary_src, &binary_dst).with_context(|| {
            format!(
                "Failed to copy binary from `{}` to `{}`",
                binary_src.display(),
                binary_dst.display()
            )
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&binary_dst)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&binary_dst, perms)?;
        }

        if !quiet {
            eprintln!(
                "{}: Installed `{}` to `{}`",
                "Success".green().bold(),
                pkg.binary_name,
                binary_dst.display()
            );
        }
    }

    Ok(0)
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
        assert!(is_git_url("file:///tmp/repo.git"));

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
