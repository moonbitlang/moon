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

//! Process-boundary transport for policy snapshots.
//!
//! Snapshot serialization and process spawning depend only on this ownership
//! protocol, not on the current temporary-file-path backend. A replacement
//! transport must preserve three operations: publish an opaque token, retain a
//! cleanup lease across intermediary processes, and consume the token exactly
//! once before the child runs untrusted code.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

use anyhow::Context;

use super::super::fs::FsPolicy;

#[derive(Debug)]
pub(super) struct SnapshotTransport {
    path: PathBuf,
    fs_policy: FsPolicy,
}

impl SnapshotTransport {
    pub(super) fn publish(contents: &[u8], fs_policy: &FsPolicy) -> anyhow::Result<Self> {
        let mut builder = tempfile::Builder::new();
        builder.prefix("moonrun-policy-").suffix(".json");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            // Create the file read-only from its first filesystem-visible
            // instant. The creator's already-open handle remains writable.
            builder.permissions(std::fs::Permissions::from_mode(0o400));
        }
        let mut snapshot = builder
            .tempfile()
            .context("failed to create temporary policy snapshot")?;
        let snapshot_path = std::fs::canonicalize(snapshot.path())
            .context("failed to resolve temporary policy snapshot")?;
        let wasi_preopen = std::fs::canonicalize(
            std::env::current_dir().context("failed to resolve the current directory")?,
        )
        .context("failed to resolve the WASI preopen directory")?;
        anyhow::ensure!(
            !snapshot_path.starts_with(&wasi_preopen),
            "temporary policy snapshot would be reachable through the WASI preopen"
        );
        snapshot
            .as_file_mut()
            .write_all(contents)
            .context("failed to write temporary policy snapshot")?;
        snapshot
            .as_file_mut()
            .flush()
            .context("failed to flush temporary policy snapshot")?;
        snapshot
            .as_file()
            .sync_all()
            .context("failed to sync temporary policy snapshot")?;

        #[cfg(windows)]
        {
            let mut permissions = snapshot
                .as_file()
                .metadata()
                .context("failed to inspect temporary policy snapshot")?
                .permissions();
            permissions.set_readonly(true);
            snapshot
                .as_file()
                .set_permissions(permissions)
                .context("failed to make temporary policy snapshot read-only")?;
        }

        let (file, _) = snapshot
            .keep()
            .context("failed to publish temporary policy snapshot")?;
        drop(file);
        fs_policy.protect_path(snapshot_path.clone());
        Ok(Self {
            path: snapshot_path,
            fs_policy: fs_policy.clone(),
        })
    }

    pub(super) fn token(&self) -> &OsStr {
        self.path.as_os_str()
    }

    pub(super) fn is_consumed(&self) -> bool {
        !self.path.exists()
    }

    pub(super) fn consume(token: OsString) -> anyhow::Result<Vec<u8>> {
        let path = PathBuf::from(token);
        let mut file = File::open(&path).context("failed to open inherited policy snapshot")?;
        remove_snapshot(&path).context("failed to consume inherited policy snapshot")?;
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)
            .context("failed to read inherited policy snapshot")?;
        Ok(contents)
    }
}

impl Drop for SnapshotTransport {
    fn drop(&mut self) {
        let _ = remove_snapshot(&self.path);
        self.fs_policy.unprotect_path(&self.path);
    }
}

#[cfg_attr(windows, allow(clippy::permissions_set_readonly_false))]
fn remove_snapshot(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        // Windows refuses to delete a file carrying its read-only attribute.
        // The snapshot remains protected by Moonrun Policy while this trusted
        // cleanup code clears the attribute and immediately removes the name.
        let mut permissions = std::fs::metadata(path)?.permissions();
        permissions.set_readonly(false);
        std::fs::set_permissions(path, permissions)?;
    }
    std::fs::remove_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> FsPolicy {
        FsPolicy::from_config(
            super::super::super::config::FsConfig {
                read: vec![PathBuf::from("*")],
                write: vec![PathBuf::from("*")],
            },
            std::path::Path::new("."),
        )
        .unwrap()
    }

    #[test]
    fn publisher_removes_snapshot_when_its_last_lease_is_dropped() {
        let policy = policy();
        let transport = SnapshotTransport::publish(b"snapshot", &policy).unwrap();
        let path = PathBuf::from(transport.token());
        assert!(
            policy
                .allows(
                    super::super::super::fs::RuntimePathBase::CurrentDirectory,
                    path.as_os_str(),
                    super::super::super::fs::FsIntents::write(),
                )
                .is_err()
        );

        drop(transport);

        assert!(!path.exists());
        policy
            .allows(
                super::super::super::fs::RuntimePathBase::CurrentDirectory,
                path.as_os_str(),
                super::super::super::fs::FsIntents::write(),
            )
            .unwrap();
    }

    #[test]
    fn consumer_unlinks_snapshot() {
        let transport = SnapshotTransport::publish(b"snapshot", &policy()).unwrap();
        let path = PathBuf::from(transport.token());

        assert_eq!(
            SnapshotTransport::consume(path.into_os_string()).unwrap(),
            b"snapshot"
        );
        assert!(transport.is_consumed());
    }
}
