//! Resolves git-sourced modules.
//!
//! Git modules reside in 2 different locations:
//! - A raw git repository is checked out into the registry cache.
//! - For each different revision of the git repository, a separate directory is created in the
//!   registry cache, and is checked out into that directory.
//!
//! This mimics the behavior of cargo's git dependencies.

use std::path::{Path, PathBuf};

use anyhow::Context;
use git2::{build::CheckoutBuilder, FetchOptions, Oid, Repository};
use moonutil::{
    hash::short_hash,
    moon_dir::{git_checkouts_dir, git_repos_dir},
    mooncakes::GitSource,
};
use url::Url;

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
    remote.fetch(&[branch], Some(&mut FetchOptions::new().depth(1)), None)?;

    let branch_ref = format!("refs/remotes/origin/{}", branch);
    let branch_ref = repo.find_reference(&branch_ref)?;
    let branch_commit = branch_ref.peel_to_commit()?;
    Ok(branch_commit.id())
}

fn pull_default_branch(repo: &Repository) -> anyhow::Result<Oid> {
    let default_branch = repo
        .find_remote("origin")?
        .default_branch()
        .context("No default branch found in remote")?;
    let default_branch = default_branch
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Malformed default branch name in remote"))?;

    pull_branch(repo, default_branch)
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
        checkout(&repo, commit, &checkout_path)?;
    }
    Ok(checkout_path)
}
