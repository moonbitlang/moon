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
    common::{FileLock, RunMode, TargetBackend},
    cond_expr::OptLevel,
    mooncakes::{ModuleName, RegistryConfig},
};
use semver::Version;
use std::path::{Path, PathBuf};

use crate::{
    cli::BuildFlags,
    rr_build::{self, BuildConfig, plan_build_from_resolved, preconfig_compile},
};

/// Represents a parsed package specification from the command line.
#[derive(Debug, Clone)]
pub struct PackageSpec {
    pub module_name: ModuleName,
    pub package_path: Option<String>,
    pub version: Option<Version>,
    pub is_wildcard: bool,
}

/// How to filter packages for installation.
enum PackageFilter {
    /// Match by filesystem path (for local/git install pointing to specific package)
    ByPath(PathBuf),
    /// Match all main packages, optionally with a prefix (for wildcard patterns)
    Wildcard { prefix: String },
    /// Match by package path string (for registry install)
    ByPackagePath(String),
}

const GIT_URL_PREFIXES: &[&str] = &["https://", "http://", "git://", "ssh://", "git@"];

/// Check if a string looks like a git URL.
pub fn is_git_url(s: &str) -> bool {
    GIT_URL_PREFIXES.iter().any(|p| s.starts_with(p))
}

/// Check if a string looks like a local filesystem path.
/// Matches: ./, ../, / (Unix absolute), C: (Windows drive letter)
pub fn is_local_path(s: &str) -> bool {
    s.starts_with("./")
        || s.starts_with("../")
        || s.starts_with('/')
        || s.chars().nth(1) == Some(':') // Windows drive letter
}

/// Yet another package path parser because we need to parse wildcard patterns.
pub fn parse_package_spec(input: &str) -> anyhow::Result<PackageSpec> {
    let (path_part, version) = if let Some(at_pos) = input.rfind('@') {
        let path = &input[..at_pos];
        let version_str = &input[at_pos + 1..];
        let version = Version::parse(version_str)
            .with_context(|| format!("Invalid version `{}`", version_str))?;
        (path, Some(version))
    } else {
        (input, None)
    };

    let (path_part, is_wildcard) = if let Some(stripped) = path_part.strip_suffix("/...") {
        (stripped, true)
    } else if let Some(stripped) = path_part.strip_suffix("...") {
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
pub fn install_binary(
    cli: &UniversalFlags,
    spec: &PackageSpec,
    install_dir: &Path,
) -> anyhow::Result<i32> {
    let quiet = cli.quiet;

    let index_dir = moonutil::moon_dir::index();
    let registry_config = RegistryConfig::load();
    let had_index = index_dir.exists();

    match mooncake::update::update(&index_dir, &registry_config) {
        Ok(_) => {
            if !quiet {
                eprintln!("{}: Updated registry index", "Info".cyan());
            }
        }
        Err(e) => {
            if had_index {
                if !quiet {
                    eprintln!(
                        "{}: Failed to update registry index, using cached index: {}",
                        "Warning".yellow().bold(),
                        e
                    );
                }
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

    if !quiet {
        eprintln!(
            "{}: Installing {}@{}",
            "Info".cyan(),
            spec.module_name,
            version
        );
    }

    let tmp_dir = tempfile::TempDir::new().context("Failed to create temporary directory")?;
    let module_dir = tmp_dir.path();

    registry.install_to(&spec.module_name, &version, module_dir, quiet)?;

    let filter = if spec.is_wildcard {
        PackageFilter::Wildcard {
            prefix: spec.package_path.clone().unwrap_or_default(),
        }
    } else {
        PackageFilter::ByPackagePath(spec.package_path.clone().unwrap_or_default())
    };

    build_and_install_packages(cli, &spec.module_name, module_dir, install_dir, filter)
}

/// Install from a local path.
pub fn install_from_local(
    cli: &UniversalFlags,
    local_path: &Path,
    install_dir: &Path,
) -> anyhow::Result<i32> {
    let input_path = dunce::canonicalize(local_path).with_context(|| {
        format!(
            "Path `{}` does not exist or cannot be resolved",
            local_path.display()
        )
    })?;

    let module_root = moonutil::dirs::find_ancestor_with_mod(&input_path).ok_or_else(|| {
        anyhow::anyhow!(
            "Path `{}` is not in a MoonBit module (no {} found in ancestors)",
            input_path.display(),
            moonutil::common::MOON_MOD_JSON
        )
    })?;

    let module = moonutil::common::read_module_desc_file_in_dir(&module_root)?;
    let module_name: ModuleName = module.name.parse().map_err(|e| anyhow::anyhow!("{}", e))?;

    let filter = if input_path == module_root {
        PackageFilter::Wildcard {
            prefix: String::new(),
        }
    } else {
        PackageFilter::ByPath(input_path)
    };

    build_and_install_packages(cli, &module_name, &module_root, install_dir, filter)
}

/// Git reference type for checkout.
pub enum GitRef<'a> {
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
pub fn install_from_git(
    cli: &UniversalFlags,
    git_url: &str,
    git_ref: GitRef<'_>,
    package_path: Option<&str>,
    install_dir: &Path,
) -> anyhow::Result<i32> {
    let quiet = cli.quiet;

    if !quiet {
        eprintln!("{}: Cloning `{}`...", "Info".cyan(), git_url);
    }

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
    let target_path = if let Some(pkg_path) = package_path {
        let pkg_path = pkg_path.trim_matches('/');
        if pkg_path.is_empty() {
            clone_dir.to_path_buf()
        } else {
            clone_dir.join(pkg_path.trim_end_matches("/..."))
        }
    } else {
        clone_dir.to_path_buf()
    };

    // Check if target path exists
    if !target_path.exists() {
        bail!(
            "Path `{}` does not exist in the repository",
            package_path.unwrap_or("")
        );
    }

    // Find module root
    let module_root = moonutil::dirs::find_ancestor_with_mod(&target_path).ok_or_else(|| {
        anyhow::anyhow!("No {} found in repository", moonutil::common::MOON_MOD_JSON)
    })?;

    let module = moonutil::common::read_module_desc_file_in_dir(&module_root)?;
    let module_name: ModuleName = module.name.parse().map_err(|e| anyhow::anyhow!("{}", e))?;

    let is_module_root = target_path == module_root;
    let is_wildcard = package_path
        .map(|p| p.ends_with("/...") || p == "...")
        .unwrap_or(is_module_root);

    let filter = if is_wildcard || is_module_root {
        PackageFilter::Wildcard {
            prefix: package_path
                .map(|p| p.trim_end_matches("/...").trim_end_matches("..."))
                .unwrap_or("")
                .to_string(),
        }
    } else {
        PackageFilter::ByPath(target_path)
    };

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

    std::fs::create_dir_all(install_dir).with_context(|| {
        format!(
            "Failed to create install directory `{}`",
            install_dir.display()
        )
    })?;

    let resolve_cfg = ResolveConfig::new_with_load_defaults(false, false, false);
    let resolve_output = moonbuild_rupes_recta::resolve(&resolve_cfg, module_dir)?;

    let main_module_id = resolve_output.local_modules()[0];
    let Some(all_pkgs) = resolve_output.pkg_dirs.packages_for_module(main_module_id) else {
        bail!("No packages found in module");
    };

    let mut packages_to_build: Vec<(PackageId, String)> = Vec::new();

    for (pkg_path, &pkg_id) in all_pkgs {
        let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
        if !pkg.raw.is_main {
            continue;
        }

        let pkg_path_str = pkg_path.to_string();

        let matched = match &filter {
            PackageFilter::ByPath(path) => pkg.root_path == *path,
            PackageFilter::Wildcard { prefix } => {
                prefix.is_empty()
                    || pkg_path_str.starts_with(&format!("{}/", prefix))
                    || pkg_path_str == *prefix
            }
            PackageFilter::ByPackagePath(target) => pkg_path_str == *target,
        };

        if matched {
            packages_to_build.push((pkg_id, pkg_path_str));
        }
    }

    if packages_to_build.is_empty() {
        match &filter {
            PackageFilter::ByPath(path) => {
                bail!(
                    "Path `{}` is not a main package (is-main: true required)",
                    path.display()
                );
            }
            PackageFilter::Wildcard { prefix } => {
                if prefix.is_empty() {
                    bail!("No main packages found in module `{}`", module_name);
                } else {
                    bail!(
                        "No main packages found matching pattern `{}/{}/...`",
                        module_name,
                        prefix
                    );
                }
            }
            PackageFilter::ByPackagePath(target) => {
                let full_name = if target.is_empty() {
                    module_name.to_string()
                } else {
                    format!("{}/{}", module_name, target)
                };
                bail!(
                    "Package `{}` not found or is not a main package (is-main: true required)",
                    full_name
                );
            }
        }
    }

    let target_dir = module_dir.join(moonutil::common::BUILD_DIR);
    std::fs::create_dir_all(&target_dir).context("Failed to create build directory")?;
    let mut installed_count = 0;

    for (pkg_id, pkg_path) in packages_to_build {
        let binary_name = pkg_path
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or(&module_name.unqual)
            .to_string();

        // Check if binary name would overwrite a reserved toolchain binary
        if moonutil::moon_dir::RESERVED_BIN_NAMES.contains(&binary_name.as_str()) {
            eprintln!(
                "{}: Cannot install `{}` - name conflicts with MoonBit toolchain binary",
                "Error".red().bold(),
                binary_name
            );
            continue;
        }

        let full_pkg_name = if pkg_path.is_empty() {
            module_name.to_string()
        } else {
            format!("{}/{}", module_name, pkg_path)
        };

        if !quiet {
            eprintln!("{}: Building `{}`...", "Info".cyan(), full_pkg_name);
        }

        let build_flags = BuildFlags {
            warn_list: Some("-a".to_string()),
            ..BuildFlags::default().with_target_backend(Some(TargetBackend::Native))
        };
        let preconfig = preconfig_compile(
            &moonutil::mooncakes::sync::AutoSyncFlags { frozen: false },
            cli,
            &build_flags,
            &target_dir,
            OptLevel::Release,
            RunMode::Build,
        );

        let (build_meta, build_graph) = plan_build_from_resolved(
            preconfig,
            &cli.unstable_feature,
            &target_dir,
            Box::new(move |_, _| Ok(vec![UserIntent::Build(pkg_id)].into())),
            resolve_output.clone(),
        )?;

        let _lock = FileLock::lock(&target_dir)?;
        rr_build::generate_all_pkgs_json(&target_dir, &build_meta, RunMode::Build)?;

        let result = rr_build::execute_build(&BuildConfig::default(), build_graph, &target_dir)?;
        if !result.successful() {
            result.print_info(quiet, "building").ok();
            eprintln!(
                "{}: Failed to build `{}`",
                "Error".red().bold(),
                full_pkg_name
            );
            continue;
        }
        result.print_info(quiet, "building").ok();

        let target = BuildTarget {
            package: pkg_id,
            kind: TargetKind::Source,
        };
        let binary_src =
            build_meta.artifacts[&BuildPlanNode::MakeExecutable(target)].artifacts[0].clone();
        let dst_name = if cfg!(windows) {
            format!("{}.exe", binary_name)
        } else {
            binary_name.clone()
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
                binary_name,
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
