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

//! This module provides an implementation of the [`GitOps`] trait by directly
//! calling the `git` command line tool.

use std::{path::PathBuf, process::Command};

use super::GitOps;

pub struct GitCommandImpl;

#[derive(Clone, Debug)]
pub struct Oid(String);

pub struct Repository(PathBuf);

impl Repository {
    pub fn path(&self) -> &PathBuf {
        &self.0
    }
}

fn run(command: &mut Command) -> Result<(), std::io::Error> {
    let output = command.output()?;
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!(
                "Command {:?} failed with status: {}\nstdout: {}\nstderr: {}",
                command, output.status, stdout, stderr
            ),
        ));
    }
    Ok(())
}

fn run_stdout(command: &mut Command) -> Result<String, std::io::Error> {
    let output = command.output()?;
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!(
                "Command {:?} failed with status: {}\nstdout: {}\nstderr: {}",
                command, output.status, stdout, stderr
            ),
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to parse stdout of command {:?}: {}", command, e),
        )
    })
}

impl GitOps for GitCommandImpl {
    type Oid = Oid;

    type Repository = Repository;

    type Error = std::io::Error;

    fn init_bare(path: &std::path::Path) -> Result<(), Self::Error> {
        let mut command = Command::new("git");
        command.arg("init").arg("--bare").arg(path);
        run(&mut command)
    }

    fn open_bare(path: &std::path::Path) -> Result<Self::Repository, Self::Error> {
        let mut command = Command::new("git");
        command
            .arg("rev-parse")
            .arg("--is-bare-repository")
            .arg(path);
        run(&mut command)?;

        Ok(Repository(path.to_path_buf()))
    }

    fn set_origin(repo: &mut Self::Repository, url: &url::Url) -> Result<(), Self::Error> {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(repo.path())
            .arg("remote")
            .arg("add")
            .arg("origin")
            .arg(url.as_str());
        run(&mut command)
    }

    fn fetch_branch(repo: &Self::Repository, branch: &str) -> Result<Self::Oid, Self::Error> {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(repo.path())
            .arg("fetch")
            .arg("origin")
            .arg(branch);
        run(&mut command)?;

        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(repo.path())
            .arg("rev-parse")
            .arg(format!("origin/{}", branch));
        let output = run_stdout(&mut command)?;

        let oid = Oid(output.trim().to_string());
        Ok(oid)
    }

    fn fetch_default_branch(repo: &Self::Repository) -> Result<Self::Oid, Self::Error> {
        // Fetch from origin to update local refs, including remote HEAD if possible
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(repo.path())
            .arg("fetch")
            .arg("origin")
            .arg("--prune"); // Clean up stale remote-tracking branches
        run(&mut command)?;

        // Ask git to automatically determine and set the remote HEAD symbolic ref locally
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(repo.path())
            .arg("remote")
            .arg("set-head")
            .arg("origin")
            .arg("--auto");
        run(&mut command)?;

        // Read the symbolic reference for origin/HEAD (e.g., refs/remotes/origin/main)
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(repo.path())
            .arg("symbolic-ref")
            .arg("refs/remotes/origin/HEAD");
        let default_branch_ref = run_stdout(&mut command)?;
        let default_branch_ref = default_branch_ref.trim();

        // Resolve the symbolic reference to a commit OID
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(repo.path())
            .arg("rev-parse")
            .arg(default_branch_ref);
        let output = run_stdout(&mut command)?;

        let oid = Oid(output.trim().to_string());
        Ok(oid)
    }

    fn fetch_revision(repo: &Self::Repository, revision: &str) -> Result<Self::Oid, Self::Error> {
        // Fetch all tags and branches first to ensure the revision object exists locally.
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(repo.path())
            .arg("fetch")
            .arg("origin")
            .arg("--prune") // Clean up stale remote-tracking branches
            .arg("--tags"); // Fetch all tags
        run(&mut command)?;

        // Now resolve the potentially truncated revision using rev-parse.
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(repo.path())
            .arg("rev-parse")
            .arg(revision); // Use the revision directly, not origin/revision
        let output = run_stdout(&mut command)?;

        let oid = Oid(output.trim().to_string());
        Ok(oid)
    }

    fn checkout(
        repo: &Self::Repository,
        commit: &Self::Oid,
        dst: &std::path::Path,
    ) -> Result<(), Self::Error> {
        let mut command = Command::new("git");
        command
            .arg("--git-dir")
            .arg(repo.path())
            .arg("--work-tree")
            .arg(dst)
            .arg("checkout")
            .arg(&commit.0)
            .arg("--force") // Force checkout, potentially overwriting files
            .arg("--") // Separator for paths
            .arg("."); // Checkout the entire tree at the specified commit
        run(&mut command)
    }

    fn oid_to_string(oid: &Self::Oid) -> String {
        oid.0.clone()
    }
}
