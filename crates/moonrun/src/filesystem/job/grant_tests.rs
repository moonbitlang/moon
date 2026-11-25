// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use super::*;
use crate::runtime::{Env, WorkingDirectory};
use std::path::Path;
use std::sync::Arc;

fn filesystem(directory: &Path, config: &str) -> HostFs {
    let path = directory.join("policy.toml");
    std::fs::write(&path, config).unwrap();
    let (policy, _) = crate::policy::load_file(&path).unwrap();
    HostFs::new(
        Some(policy.filesystem()),
        Arc::new(Env::ambient()),
        Arc::new(WorkingDirectory::Fixed(directory.to_owned())),
    )
}

fn open(
    fs: &HostFs,
    parent: Option<ResourceRef>,
    path: &str,
    access: i32,
) -> AsyncHostResult<ResourceRef> {
    let mut job = Job::open(
        parent,
        path.into(),
        access,
        0,
        false,
        0,
        0o600,
        STAT_OPEN_IDENTITY,
        32,
    )?;
    job.run(fs)?;
    let Kind::Open {
        result: Some(result),
        ..
    } = job.kind
    else {
        panic!("open result")
    };
    let ResourcePublication::Unpublished(resource) = result.resource else {
        panic!("resource")
    };
    Ok(Arc::new(resource))
}

#[test]
fn directory_entry_metadata_respects_the_resolved_child_name() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("work/private")).unwrap();
    let alternate = dir.path().join("work/PRIVATE");
    if !alternate.exists() {
        return; // The volume is case-sensitive, so this spelling names no entry.
    }
    for config in [
        "[dirs]\nwork = ['write']\n'work/private' = []",
        "[fs]\nread = ['work', { glob = '!work/private' }]",
    ] {
        let fs = filesystem(dir.path(), config);
        assert!(matches!(
            fs.authorize(crate::filesystem::FsOperation::InspectEntry(FsPath::Cwd(
                alternate.as_os_str()
            ))),
            Err(crate::filesystem::FsError::Denied)
        ));
    }
}

#[test]
fn data_metadata_locks_and_flush_share_the_captured_rights() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("file"), b"keep").unwrap();
    for (access, read, write) in [(0, true, false), (1, false, true), (2, true, true)] {
        let config = format!(
            "[fs]\nread = {}\nwrite = {}",
            if read { "['file']" } else { "[]" },
            if write { "['file']" } else { "[]" }
        );
        let fs = filesystem(dir.path(), &config);
        let file = open(&fs, None, "file", access).unwrap();
        for result in [
            Job::read(Arc::clone(&file), 4, 0).run(&fs).map(|_| ()),
            Job::file_size(Arc::clone(&file)).run(&fs).map(|_| ()),
            fs.try_lock_resource(&file, false),
        ] {
            assert_eq!(
                result,
                if read {
                    Ok(())
                } else {
                    Err(AsyncHostError::PermissionDenied)
                }
            );
        }
        if read {
            fs.unlock_resource(&file).unwrap();
        }
        for result in [
            Job::write(Arc::clone(&file), b"new!".to_vec(), 0)
                .run(&fs)
                .map(|_| ()),
            Job::fsync(Arc::clone(&file), false).run(&fs).map(|_| ()),
            fs.try_lock_resource(&file, true),
        ] {
            assert_eq!(
                result,
                if write {
                    Ok(())
                } else {
                    Err(AsyncHostError::PermissionDenied)
                }
            );
        }
        // Basic identity is already supplied by opening, even without read.
        assert!(fs.resource_kind(&file).is_ok());
    }
}

#[cfg(unix)]
#[test]
fn replacing_the_opening_name_cannot_revoke_or_expand_a_grant() {
    let dir = tempfile::tempdir().unwrap();
    for read in [true, false] {
        std::fs::write(dir.path().join("file"), b"keep").unwrap();
        std::fs::write(dir.path().join("other"), b"replacement").unwrap();
        let fs = filesystem(
            dir.path(),
            if read {
                "[fs]\nread = ['file']\nwrite = ['file']"
            } else {
                "[fs]\nread = ['other']\nwrite = ['file']"
            },
        );
        let file = open(&fs, None, "file", if read { 2 } else { 1 }).unwrap();
        std::fs::rename(dir.path().join("file"), dir.path().join("moved")).unwrap();
        std::os::unix::fs::symlink("other", dir.path().join("file")).unwrap();
        let mut size = Job::file_size(Arc::clone(&file));
        if read {
            size.run(&fs).unwrap();
            assert_eq!(size.file_size_result().unwrap(), 4);
            let mut job = Job::read(Arc::clone(&file), 4, 0);
            assert_eq!(job.run(&fs).unwrap(), 4);
            let mut bytes = [0; 4];
            job.copy_read_result(0, &mut bytes[..], 0, 0, 4).unwrap();
            assert_eq!(&bytes, b"keep");
        } else {
            assert_eq!(size.run(&fs), Err(AsyncHostError::PermissionDenied));
        }
        fs.try_lock_resource(&file, true).unwrap();
        assert_eq!(
            Job::write(Arc::clone(&file), b"new!".to_vec(), 0)
                .run(&fs)
                .unwrap(),
            4
        );
        Job::fsync(file, false).run(&fs).unwrap();
        assert_eq!(std::fs::read(dir.path().join("moved")).unwrap(), b"new!");
        assert_eq!(
            std::fs::read(dir.path().join("other")).unwrap(),
            b"replacement"
        );
        // A new acquisition still checks the current resolved pathname.
        assert_eq!(open(&fs, None, "file", 0).is_ok(), !read);
        std::fs::remove_file(dir.path().join("file")).unwrap();
    }
}

#[test]
fn directory_children_recheck_path_rules_instead_of_inheriting_the_grant() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("work/private")).unwrap();
    std::fs::write(dir.path().join("work/private/file"), b"keep").unwrap();
    let fs = filesystem(
        dir.path(),
        "[fs]\nread = ['work']\nwrite = ['work', { glob = '!work/private/**' }]",
    );
    let parent = open(&fs, None, "work", 3).unwrap();
    // Directory::open normalizes separators before submitting a Job.
    let child = ["private", "file"].join(std::path::MAIN_SEPARATOR_STR);
    open(&fs, Some(Arc::clone(&parent)), &child, 0).unwrap();
    for (base, path) in [(None, "work/private/file"), (Some(parent), child.as_str())] {
        assert!(matches!(
            open(&fs, base, path, 1),
            Err(AsyncHostError::PermissionDenied)
        ));
    }
}

#[test]
fn capturing_full_policy_access_does_not_widen_the_os_open_mode() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("file"), b"keep").unwrap();
    let fs = filesystem(dir.path(), "[fs]\nread = ['file']\nwrite = ['file']");
    let file = open(&fs, None, "file", 1).unwrap();
    let mut size = Job::file_size(Arc::clone(&file));
    size.run(&fs).unwrap();
    assert_eq!(size.file_size_result().unwrap(), 4);
    assert!(Job::read(Arc::clone(&file), 4, 0).run(&fs).is_err());
    assert_eq!(Job::write(file, b"new!".to_vec(), 0).run(&fs).unwrap(), 4);
}

#[test]
fn captured_access_intersects_all_policy_sources() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("file"), b"keep").unwrap();
    let path = dir.path().join("policy.toml");
    std::fs::write(&path, "[fs]\nread = ['file']\nwrite = ['file']").unwrap();
    let (first, _) = crate::policy::load_file(&path).unwrap();
    std::fs::write(&path, "[fs]\nwrite = ['file']").unwrap();
    let (second, _) = crate::policy::load_file(&path).unwrap();
    let fs = HostFs::new(
        Some(first.intersect(second).filesystem()),
        Arc::new(Env::ambient()),
        Arc::new(WorkingDirectory::Fixed(dir.path().to_owned())),
    );
    let file = open(&fs, None, "file", 1).unwrap();
    assert_eq!(
        Job::file_size(Arc::clone(&file)).run(&fs),
        Err(AsyncHostError::PermissionDenied)
    );
    assert_eq!(Job::write(file, b"new!".to_vec(), 0).run(&fs).unwrap(), 4);
}
