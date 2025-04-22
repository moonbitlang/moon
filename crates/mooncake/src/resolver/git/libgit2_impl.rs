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

#![cfg(feature = "libgit2")]
//! This module provides an implementation of the [`GitOps`] trait using the `libgit2` library.

use git2::{build::CheckoutBuilder, FetchOptions, Oid, Repository};

use super::GitOps;

pub struct LibGit2Impl;

impl GitOps for LibGit2Impl {
    type Oid = Oid;

    type Repository = Repository;

    type Error = git2::Error;

    fn init_bare(path: &std::path::Path) -> Result<(), Self::Error> {
        Repository::init_bare(path).map(|_| ())
    }

    fn open_bare(path: &std::path::Path) -> Result<Self::Repository, Self::Error> {
        Repository::open_bare(path)
    }

    fn set_origin(repo: &mut Self::Repository, url: &url::Url) -> Result<(), Self::Error> {
        repo.remote("origin", url.as_str()).map(|_| ())
    }

    fn fetch_branch(repo: &Self::Repository, branch: &str) -> Result<Self::Oid, Self::Error> {
        let mut remote = repo.find_remote("origin")?;
        if !remote.connected() {
            remote.connect(git2::Direction::Fetch)?;
        }

        let mut fetch_options = FetchOptions::new();
        remote.fetch(&[branch], Some(&mut fetch_options), None)?;

        let branch_ref = format!("refs/remotes/origin/{}", branch);
        let branch_ref = repo.find_reference(&branch_ref)?;
        let branch_commit = branch_ref.peel_to_commit()?;
        Ok(branch_commit.id())
    }
    fn fetch_default_branch(repo: &Self::Repository) -> Result<Self::Oid, Self::Error> {
        let mut remote = repo.find_remote("origin")?;

        // Try to get default branch, connecting if necessary
        let default_branch = match remote.default_branch() {
            Ok(branch) => branch,
            Err(_) => {
                remote.connect(git2::Direction::Fetch)?;
                remote.default_branch()?
            }
        };

        // Extract branch name
        let default_branch = default_branch
            .as_str()
            .ok_or_else(|| git2::Error::from_str("Malformed default branch name in remote"))?;
        let default_branch = default_branch.trim_start_matches("refs/heads/");

        // Fetch the branch
        let result = Self::fetch_branch(repo, default_branch);

        // Disconnect and return
        remote.disconnect()?;
        result
    }

    fn fetch_revision(repo: &Self::Repository, revision: &str) -> Result<Self::Oid, Self::Error> {
        let mut remote = repo.find_remote("origin")?;
        remote.fetch(&[revision], Some(FetchOptions::new().depth(1)), None)?;
        let commit = repo.revparse_single(revision)?;
        Ok(commit.id())
    }

    fn checkout(
        repo: &Self::Repository,
        commit: &Self::Oid,
        dst: &std::path::Path,
    ) -> Result<(), Self::Error> {
        let commit = repo.find_commit(commit.clone())?;
        let tree = commit.tree()?;
        repo.checkout_tree(
            &tree.into_object(),
            Some(CheckoutBuilder::new().target_dir(dst)),
        )?;
        Ok(())
    }

    fn oid_to_string(oid: &Self::Oid) -> String {
        oid.to_string()
    }
}
