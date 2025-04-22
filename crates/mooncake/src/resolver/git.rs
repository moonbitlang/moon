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

//! Resolves git-sourced modules.
//!
//! Git modules reside in 2 different locations:
//! - A raw git repository is checked out into the registry cache.
//! - For each different revision of the git repository, a separate directory is created in the
//!   registry cache, and is checked out into that directory.
//!
//! This mimics the behavior of cargo's git dependencies.

use std::{
    path::{Path, PathBuf},
    rc::Rc,
};

use anyhow::Context;
use moonutil::{
    common::{read_module_from_json, MOON_MOD_JSON},
    hash::short_hash,
    module::MoonMod,
    moon_dir::{git_checkouts_dir, git_dir, git_repos_dir},
    mooncakes::GitSource,
};
use url::Url;
use walkdir::WalkDir;

mod command_impl;
pub use command_impl::GitCommandImpl;

mod libgit2_impl;

#[cfg(feature = "libgit2")]
pub use libgit2_impl::LibGit2Impl;

/// The default git operations implementation.
#[cfg(feature = "libgit2")]
pub type DefaultGitOps = LibGit2Impl;
/// The default git operations implementation.
#[cfg(not(feature = "libgit2"))]
pub type DefaultGitOps = GitCommandImpl;

/// The git operations used in the resolver.
pub trait GitOps {
    type Oid: Clone + std::fmt::Debug + Send + Sync + 'static;
    type Repository;
    type Error: std::error::Error + Send + Sync + 'static;

    fn init_bare(path: &Path) -> Result<(), Self::Error>;
    fn open_bare(path: &Path) -> Result<Self::Repository, Self::Error>;
    fn set_origin(repo: &mut Self::Repository, url: &Url) -> Result<(), Self::Error>;
    fn fetch_branch(repo: &Self::Repository, branch: &str) -> Result<Self::Oid, Self::Error>;
    fn fetch_default_branch(repo: &Self::Repository) -> Result<Self::Oid, Self::Error>;
    fn fetch_revision(repo: &Self::Repository, revision: &str) -> Result<Self::Oid, Self::Error>;
    fn checkout(repo: &Self::Repository, commit: &Self::Oid, dst: &Path)
        -> Result<(), Self::Error>;
    fn oid_to_string(oid: &Self::Oid) -> String;
}

fn ident(url: &Url) -> &str {
    url.path_segments()
        .and_then(|s| s.last())
        .unwrap_or("")
        .trim_end_matches(".git")
}

fn repo_name(url: &Url) -> String {
    let id = ident(url);
    let hash = short_hash(url);
    format!("{}-{:016x}", id, hash)
}

fn repo_path(url: &Url) -> PathBuf {
    git_repos_dir().join(repo_name(url))
}

fn repo_checkout_path<G: GitOps>(url: &Url, commit: &G::Oid) -> PathBuf {
    git_checkouts_dir()
        .join(repo_name(url))
        .join(G::oid_to_string(&commit))
}

pub fn init_repo_dir<G: GitOps>(url: &Url) -> anyhow::Result<(PathBuf, G::Repository)> {
    let path = repo_path(url);
    std::fs::create_dir_all(&path).context("failed to create git repository directory")?;

    G::init_bare(&path).context("failed to initialize git repository")?;
    let mut repo = G::open_bare(&path).context("failed to open git repository")?;
    G::set_origin(&mut repo, url).context("failed to set git origin")?;

    Ok((path, repo))
}
pub fn open_or_init_repo_dir<G: GitOps>(url: &Url) -> anyhow::Result<(PathBuf, G::Repository)>
where
{
    let path = repo_path(url);
    if path.exists() {
        let repo = G::open_bare(&path).context("failed to open git repository")?;
        Ok((path, repo))
    } else {
        init_repo_dir::<G>(url)
    }
}

pub fn resolve<G: GitOps>(source: &GitSource, moon_home: &Path) -> anyhow::Result<PathBuf> {
    // Lock the cache directory to prevent concurrent access.
    let git_dir = git_dir(moon_home);
    std::fs::create_dir_all(&git_dir)
        .context("Unable to create the parent directory for git operations")?;
    let _lock =
        moonutil::common::FileLock::lock(&git_dir).context("Unable to lock the git directory")?;

    let source_url = Url::parse(&source.url).context("Malformed git source url")?;
    // Open or initialize the repository.
    let (_, repo) = open_or_init_repo_dir::<G>(&source_url)?;
    // Find the revision to checkout.
    let commit = if let Some(branch) = &source.branch {
        G::fetch_branch(&repo, branch).context("Failed to fetch branch")?
    } else if let Some(revision) = &source.revision {
        G::fetch_revision(&repo, revision).context("Failed to fetch revision")?
    } else {
        G::fetch_default_branch(&repo).context("Failed to fetch default branch")?
    };
    // Checkout the revision to the cache.
    let checkout_path = repo_checkout_path::<G>(&source_url, &commit);
    if !checkout_path.exists() {
        std::fs::create_dir_all(&checkout_path)?;
        G::checkout(&repo, &commit, &checkout_path).context("Failed to checkout")?;
    }

    drop(_lock);
    Ok(checkout_path)
}

pub fn recursively_scan_for_moon_mods(path: &Path) -> anyhow::Result<Vec<(PathBuf, Rc<MoonMod>)>> {
    let mut mods = Vec::new();
    for entry in WalkDir::new(path) {
        let entry = entry?;
        if entry.file_name() == MOON_MOD_JSON {
            let dir = entry.path().parent().unwrap();
            let module = read_module_from_json(entry.path()).context("Failed to read module")?;
            mods.push((dir.into(), Rc::new(module)));
        }
    }
    Ok(mods)
}

#[cfg(test)]
mod test {
    use std::path::Path;
    use std::process::Command;

    use expect_test::expect;
    use walkdir::WalkDir;

    use crate::resolver::git::DefaultGitOps;

    const SAMPLE_GIT_REPO_DIR: &str = "tests/git_test_template";

    #[cfg(feature = "libgit2")]
    pub fn make_sample_git_repo(path: &Path) {
        // cp -r crates/mooncake/test/git_test_template/* repo/
        copy_dir::copy_dir(SAMPLE_GIT_REPO_DIR, path).unwrap();

        // git init .
        let repo = git2::Repository::init(path).unwrap();

        // config user.{name,email}
        repo.config()
            .unwrap()
            .set_str("user.name", "mooncake-tester")
            .unwrap();
        repo.config()
            .unwrap()
            .set_str("user.email", "me@example.com")
            .unwrap();

        // git add .
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();

        // git commit -m "Initial commit"
        let head = repo
            .commit(
                Some("HEAD"),
                &repo.signature().unwrap(),
                &repo.signature().unwrap(),
                "Initial commit",
                &tree,
                &[],
            )
            .unwrap();

        // git branch main
        let commit = repo.find_commit(head).unwrap();
        repo.branch("main", &commit, false).unwrap();
        repo.set_head("refs/heads/main").unwrap();

        // generate another branch with a different file

        // git checkout -b other
        repo.branch("other", &commit, false).unwrap();
        repo.set_head("refs/heads/other").unwrap();

        // echo "other file" > other_file
        let other_path = path.join("other_file");
        std::fs::write(&other_path, "other file").unwrap();

        // git add other_file
        index.add_path(Path::new("other_file")).unwrap();
        index.write().unwrap();

        // git commit -m "Add other file"
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(
            Some("HEAD"),
            &repo.signature().unwrap(),
            &repo.signature().unwrap(),
            "Add other file",
            &tree,
            &[&commit],
        )
        .unwrap();

        // set default branch back to main
        repo.set_head("refs/heads/main").unwrap();
    }

    #[cfg(not(feature = "libgit2"))]
    fn run_git_command(args: &[&str], cwd: &Path) {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .expect("Failed to execute git command");
        if !output.status.success() {
            panic!(
                "Git command failed: {:?}\nStdout: {}\nStderr: {}",
                args,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    #[cfg(not(feature = "libgit2"))]
    pub fn make_sample_git_repo(path: &Path) {
        // cp -r crates/mooncake/test/git_test_template/* repo/
        copy_dir::copy_dir(SAMPLE_GIT_REPO_DIR, path).unwrap();

        // git init .
        run_git_command(&["init"], path);
        // use `master` as the default branch
        run_git_command(&["branch", "-m", "master"], path);

        // config user.{name,email}
        run_git_command(&["config", "user.name", "mooncake-tester"], path);
        run_git_command(&["config", "user.email", "me@example.com"], path);

        // git add .
        run_git_command(&["add", "."], path);

        // git commit -m "Initial commit"
        run_git_command(&["commit", "-m", "Initial commit"], path);

        // git branch main
        run_git_command(&["branch", "main"], path);
        // git checkout main (to set HEAD implicitly)
        run_git_command(&["checkout", "main"], path);

        // generate another branch with a different file

        // git checkout -b other
        run_git_command(&["checkout", "-b", "other"], path);

        // echo "other file" > other_file
        let other_path = path.join("other_file");
        std::fs::write(&other_path, "other file").unwrap();

        // git add other_file
        run_git_command(&["add", "other_file"], path);

        // git commit -m "Add other file"
        run_git_command(&["commit", "-m", "Add other file"], path);

        // set default branch back to main
        run_git_command(&["checkout", "main"], path);

        // git tag v1.0.0
        run_git_command(&["tag", "v1.0.0"], path);
    }

    fn list_dir_contents(dir: &Path) -> String {
        WalkDir::new(dir)
            .sort_by_file_name()
            .into_iter()
            .map(|e| {
                e.unwrap()
                    .path()
                    .strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_basic_git_resolver() {
        let temp_dir = tempfile::tempdir().unwrap();
        let repo_path = temp_dir.path().join("repo");
        make_sample_git_repo(&repo_path);

        let test_moon_home = temp_dir.path().join("moon_home");
        std::fs::create_dir_all(&test_moon_home).unwrap();
        std::env::set_var("MOON_HOME", test_moon_home);

        // Try to resolve the git repository.
        let source = moonutil::mooncakes::GitSource {
            url: format!("file://{}", repo_path.display()),
            branch: None,
            revision: None,
        };

        let checkout = super::resolve::<DefaultGitOps>(&source).unwrap();
        assert!(checkout.exists());

        // Check that the checkout is correct.
        let dir_contents = list_dir_contents(&checkout);
        expect![[r#"

            README.md
            lib
            lib/hello.mbt
            lib/moon.pkg.json
            moon.mod.json
            nonroot_pkg
            nonroot_pkg/README.md
            nonroot_pkg/lib
            nonroot_pkg/lib/hello.mbt
            nonroot_pkg/lib/moon.pkg.json
            nonroot_pkg/moon.mod.json"#]]
        .assert_eq(&dir_contents);
    }

    #[test]
    fn test_git_with_branch_specified() {
        let temp_dir = tempfile::tempdir().unwrap();
        let repo_path = temp_dir.path().join("repo");
        make_sample_git_repo(&repo_path);

        let test_moon_home = temp_dir.path().join("moon_home");
        std::fs::create_dir_all(&test_moon_home).unwrap();
        std::env::set_var("MOON_HOME", test_moon_home);

        // Try to resolve the git repository.
        let source = moonutil::mooncakes::GitSource {
            url: format!("file://{}", repo_path.display()),
            branch: Some("other".into()),
            revision: None,
        };

        let checkout = super::resolve::<DefaultGitOps>(&source).unwrap();
        assert!(checkout.exists());

        // Check that the checkout is correct.
        let dir_contents = list_dir_contents(&checkout);
        expect![[r#"

            README.md
            lib
            lib/hello.mbt
            lib/moon.pkg.json
            moon.mod.json
            nonroot_pkg
            nonroot_pkg/README.md
            nonroot_pkg/lib
            nonroot_pkg/lib/hello.mbt
            nonroot_pkg/lib/moon.pkg.json
            nonroot_pkg/moon.mod.json
            other_file"#]]
        .assert_eq(&dir_contents);
    }

    #[test]
    fn test_find_all_packages() {
        let temp_dir = tempfile::tempdir().unwrap();
        let repo_path = temp_dir.path().join("repo");
        make_sample_git_repo(&repo_path);

        let test_moon_home = temp_dir.path().join("moon_home");
        std::fs::create_dir_all(&test_moon_home).unwrap();
        std::env::set_var("MOON_HOME", test_moon_home);

        // Try to resolve the git repository.
        let source = moonutil::mooncakes::GitSource {
            url: format!("file://{}", repo_path.display()),
            branch: None,
            revision: None,
        };

        let checkout = super::resolve::<DefaultGitOps>(&source).unwrap();
        assert!(checkout.exists());

        // Check that the checkout is correct.
        let mods = super::recursively_scan_for_moon_mods(&checkout).unwrap();
        let mods = mods
            .into_iter()
            .map(|(_, module)| module.name.clone())
            .collect::<Vec<_>>();
        assert_eq!(mods, vec!["testing/test", "testing/nonroot-test"]);
    }

    #[test]
    fn test_git_with_revision_specified() {
        let temp_dir = tempfile::tempdir().unwrap();
        let repo_path = temp_dir.path().join("repo");
        make_sample_git_repo(&repo_path);

        let test_moon_home = temp_dir.path().join("moon_home");
        std::fs::create_dir_all(&test_moon_home).unwrap();
        std::env::set_var("MOON_HOME", test_moon_home);

        // Get the commit hash of the main branch
        let output = Command::new("git")
            .args(["rev-parse", "main"])
            .current_dir(&repo_path)
            .output()
            .expect("Failed to get main branch revision");
        assert!(output.status.success());
        let revision = String::from_utf8(output.stdout).unwrap().trim().to_string();

        // Try to resolve the git repository using the revision.
        let source = moonutil::mooncakes::GitSource {
            url: format!("file://{}", repo_path.display()),
            branch: None,
            revision: Some(revision),
        };

        let checkout = super::resolve::<DefaultGitOps>(&source).unwrap();
        assert!(checkout.exists());

        // Check that the checkout is correct (should be the same as default branch).
        let dir_contents = list_dir_contents(&checkout);
        expect![[r#"

            .gitignore
            README.md
            lib
            lib/hello.mbt
            lib/moon.pkg.json
            moon.mod.json
            nonroot_pkg
            nonroot_pkg/README.md
            nonroot_pkg/lib
            nonroot_pkg/lib/hello.mbt
            nonroot_pkg/lib/moon.pkg.json
            nonroot_pkg/moon.mod.json"#]]
        .assert_eq(&dir_contents);
    }

    #[test]
    fn test_git_with_non_existent_branch() {
        let temp_dir = tempfile::tempdir().unwrap();
        let repo_path = temp_dir.path().join("repo");
        make_sample_git_repo(&repo_path);

        let test_moon_home = temp_dir.path().join("moon_home");
        std::fs::create_dir_all(&test_moon_home).unwrap();
        std::env::set_var("MOON_HOME", test_moon_home);

        // Try to resolve the git repository using a non-existent branch.
        let source = moonutil::mooncakes::GitSource {
            url: format!("file://{}", repo_path.display()),
            branch: Some("non-existent-branch".into()),
            revision: None,
        };

        let result = super::resolve::<DefaultGitOps>(&source);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to fetch branch"));
    }

    #[test]
    fn test_git_with_non_existent_revision() {
        let temp_dir = tempfile::tempdir().unwrap();
        let repo_path = temp_dir.path().join("repo");
        make_sample_git_repo(&repo_path);

        let test_moon_home = temp_dir.path().join("moon_home");
        std::fs::create_dir_all(&test_moon_home).unwrap();
        std::env::set_var("MOON_HOME", test_moon_home);

        // Try to resolve the git repository using a non-existent revision.
        let source = moonutil::mooncakes::GitSource {
            url: format!("file://{}", repo_path.display()),
            branch: None,
            revision: Some("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".into()), // Highly unlikely revision
        };

        let result = super::resolve::<DefaultGitOps>(&source);
        assert!(result.is_err());
        eprintln!("error is: {}", result.unwrap_err());
    }

    #[test]
    fn test_git_with_non_existent_repo() {
        let temp_dir = tempfile::tempdir().unwrap();
        let test_moon_home = temp_dir.path().join("moon_home");
        std::fs::create_dir_all(&test_moon_home).unwrap();
        std::env::set_var("MOON_HOME", test_moon_home);

        // Try to resolve a git repository URL that does not exist.
        let source = moonutil::mooncakes::GitSource {
            url: "file:///path/that/definitely/does/not/exist".into(),
            branch: None,
            revision: None,
        };

        let result = super::resolve::<DefaultGitOps>(&source);
        assert!(result.is_err());
        eprintln!("error is: {}", result.unwrap_err());
    }

    #[test]
    fn test_git_with_truncated_revision() {
        let temp_dir = tempfile::tempdir().unwrap();
        let repo_path = temp_dir.path().join("repo");
        make_sample_git_repo(&repo_path);

        let test_moon_home = temp_dir.path().join("moon_home");
        std::fs::create_dir_all(&test_moon_home).unwrap();
        std::env::set_var("MOON_HOME", test_moon_home);

        // Get the full commit hash of the main branch
        let output = Command::new("git")
            .args(["rev-parse", "main"])
            .current_dir(&repo_path)
            .output()
            .expect("Failed to get main branch revision");
        assert!(output.status.success());
        let full_revision = String::from_utf8(output.stdout).unwrap().trim().to_string();
        let truncated_revision = full_revision[..7].to_string(); // Use first 7 chars

        // Try to resolve the git repository using the truncated revision.
        let source = moonutil::mooncakes::GitSource {
            url: format!("file://{}", repo_path.display()),
            branch: None,
            revision: Some(truncated_revision),
        };

        let checkout = super::resolve::<DefaultGitOps>(&source).unwrap();
        assert!(checkout.exists());

        // Check that the checkout is correct (should be the same as default branch).
        let dir_contents = list_dir_contents(&checkout);
        expect![[r#"

            .gitignore
            README.md
            lib
            lib/hello.mbt
            lib/moon.pkg.json
            moon.mod.json
            nonroot_pkg
            nonroot_pkg/README.md
            nonroot_pkg/lib
            nonroot_pkg/lib/hello.mbt
            nonroot_pkg/lib/moon.pkg.json
            nonroot_pkg/moon.mod.json"#]]
        .assert_eq(&dir_contents);
    }

    #[test]
    fn test_git_with_tag_specified() {
        let temp_dir = tempfile::tempdir().unwrap();
        let repo_path = temp_dir.path().join("repo");
        make_sample_git_repo(&repo_path);

        let test_moon_home = temp_dir.path().join("moon_home");
        std::fs::create_dir_all(&test_moon_home).unwrap();
        std::env::set_var("MOON_HOME", test_moon_home);

        // Try to resolve the git repository using the tag.
        let source = moonutil::mooncakes::GitSource {
            url: format!("file://{}", repo_path.display()),
            branch: None,
            revision: Some("v1.0.0".into()), // Use the tag created in make_sample_git_repo
        };

        let checkout = super::resolve::<DefaultGitOps>(&source).unwrap();
        assert!(checkout.exists());

        // Check that the checkout is correct (should be the same as default branch).
        let dir_contents = list_dir_contents(&checkout);
        expect![[r#"

            .gitignore
            README.md
            lib
            lib/hello.mbt
            lib/moon.pkg.json
            moon.mod.json
            nonroot_pkg
            nonroot_pkg/README.md
            nonroot_pkg/lib
            nonroot_pkg/lib/hello.mbt
            nonroot_pkg/lib/moon.pkg.json
            nonroot_pkg/moon.mod.json"#]]
        .assert_eq(&dir_contents);
    }
}
