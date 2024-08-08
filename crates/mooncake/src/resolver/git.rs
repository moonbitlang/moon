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
use git2::{build::CheckoutBuilder, FetchOptions, Oid, Repository};
use moonutil::{
    common::{read_module_desc_file_in_dir, read_module_from_json, MOON_MOD_JSON},
    hash::short_hash,
    module::MoonMod,
    moon_dir::{git_checkouts_dir, git_repos_dir},
    mooncakes::GitSource,
};
use url::Url;
use walkdir::WalkDir;

fn ident(url: &Url) -> &str {
    url.path_segments()
        .and_then(|s| s.last())
        .unwrap_or("")
        .trim_end_matches(".git")
}

fn repo_name(url: &Url) -> String {
    let id = ident(url);
    let hash = short_hash(url);
    format!("{}-{}", id, hash)
}

fn repo_path(url: &Url) -> PathBuf {
    git_repos_dir().join(repo_name(url))
}

fn repo_checkout_path(url: &Url, commit: Oid) -> PathBuf {
    git_checkouts_dir()
        .join(repo_name(url))
        .join(commit.to_string())
}

pub fn init_repo_dir(url: &Url) -> anyhow::Result<(PathBuf, Repository)> {
    let path = repo_path(url);
    std::fs::create_dir_all(&path).context("failed to create git repository directory")?;
    git2::Repository::init_bare(&path).context("failed to initialize git repository")?;
    let repo = git2::Repository::open_bare(&path).context("failed to open git repository")?;
    repo.remote("origin", url.as_str())?;

    Ok((path, repo))
}

pub fn open_or_init_repo_dir(url: &Url) -> anyhow::Result<(PathBuf, Repository)> {
    let path = repo_path(url);
    if path.exists() {
        let repo = git2::Repository::open_bare(&path).context("failed to open git repository")?;
        Ok((path, repo))
    } else {
        init_repo_dir(url)
    }
}

fn pull_branch(repo: &Repository, branch: &str) -> anyhow::Result<Oid> {
    let mut remote = repo.find_remote("origin")?;
    if !remote.connected() {
        remote.connect(git2::Direction::Fetch)?;
    }

    remote
        .fetch(&[branch], None, None)
        .context("Failed to fetch from remote")?;

    let branch_ref = format!("refs/remotes/origin/{}", branch);
    let branch_ref = repo.find_reference(&branch_ref)?;
    let branch_commit = branch_ref.peel_to_commit()?;
    Ok(branch_commit.id())
}

fn pull_default_branch(repo: &Repository) -> anyhow::Result<Oid> {
    let mut remote = repo.find_remote("origin")?;
    // if the default branch is not set, we connect to the remote and try again
    let default_branch = if let Ok(default_branch) = remote.default_branch() {
        default_branch
    } else {
        remote.connect(git2::Direction::Fetch)?;
        remote
            .default_branch()
            .context("Failed to get default branch")?
    };
    let default_branch = default_branch
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Malformed default branch name in remote"))?;
    let default_branch = default_branch.trim_start_matches("refs/heads/");
    let res = pull_branch(repo, default_branch);
    remote.disconnect()?;
    res
}

fn pull_specific_revision(repo: &Repository, revision: &str) -> anyhow::Result<Oid> {
    let mut remote = repo.find_remote("origin")?;
    remote.fetch(&[revision], Some(FetchOptions::new().depth(1)), None)?;
    let commit = repo.revparse_single(revision)?;
    Ok(commit.id())
}

fn checkout(repo: &Repository, commit: Oid, dst: &Path) -> anyhow::Result<()> {
    let commit = repo.find_commit(commit)?;
    let tree = commit.tree()?;
    repo.checkout_tree(
        &tree.into_object(),
        Some(CheckoutBuilder::new().target_dir(dst)),
    )?;
    Ok(())
}

pub fn resolve(source: &GitSource) -> anyhow::Result<PathBuf> {
    let source_url = Url::parse(&source.url).context("Malformed git source url")?;
    // Open or initialize the repository.
    let (_, repo) = open_or_init_repo_dir(&source_url)?;
    // Find the revision to checkout.
    let commit = if let Some(branch) = &source.branch {
        pull_branch(&repo, branch)?
    } else if let Some(revision) = &source.revision {
        pull_specific_revision(&repo, revision)?
    } else {
        pull_default_branch(&repo)?
    };
    // Checkout the revision to the cache.
    let checkout_path = repo_checkout_path(&source_url, commit);
    if !checkout_path.exists() {
        std::fs::create_dir_all(&checkout_path)?;
        checkout(&repo, commit, &checkout_path)?;
    }
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

    use expect_test::expect;
    use walkdir::WalkDir;

    const SAMPLE_GIT_REPO_DIR: &str = "tests/git_test_template";

    fn make_sample_git_repo(path: &Path) {
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

        let checkout = super::resolve(&source).unwrap();
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

        let checkout = super::resolve(&source).unwrap();
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

        let checkout = super::resolve(&source).unwrap();
        assert!(checkout.exists());

        // Check that the checkout is correct.
        let mods = super::recursively_scan_for_moon_mods(&checkout).unwrap();
        let mods = mods
            .into_iter()
            .map(|(_, module)| module.name.clone())
            .collect::<Vec<_>>();
        assert_eq!(mods, vec!["testing/test", "testing/nonroot-test"]);
    }
}
