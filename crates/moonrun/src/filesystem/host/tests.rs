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
#[cfg(unix)]
use crate::filesystem::FsTarget;
use crate::filesystem::{FsOperation, FsPath};
use crate::policy::{self, Policy};
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::path::Path;

fn host_fs(policy: Option<Policy>, environment: Arc<Env>) -> HostFs {
    HostFs::new(
        policy.as_ref().map(Policy::filesystem),
        environment,
        Arc::new(WorkingDirectory::Ambient),
    )
}

#[cfg(unix)]
#[test]
fn builtin_timezone_grant_is_read_only_and_excludes_untrusted_aliases() {
    let host = HostFs::new(
        Some(Default::default()),
        Arc::new(Env::ambient()),
        Arc::new(WorkingDirectory::Ambient),
    );
    let file = Path::new("/usr/share/zoneinfo/UTC");
    if !file.is_file() {
        return;
    } // Minimal installations may omit tzdata.
    assert!(host.read_file(file).unwrap().starts_with(b"TZif"));
    let path = FsPath::Cwd(file.as_os_str());
    for access in [0, 1] {
        let granted = host
            .authorize(FsOperation::AccessCheck { path, access })
            .unwrap();
        assert_eq!(granted, crate::policy::AccessRights::READ);
    }
    for operation in [
        FsOperation::AccessCheck { path, access: 2 },
        FsOperation::AccessCheck { path, access: 3 },
        FsOperation::Write(path),
        FsOperation::ChangeEntry(path),
        FsOperation::Watch(FsTarget::Path(path)),
        FsOperation::OpenAsync {
            path,
            access: 0,
            create_mode: 1,
            read_metadata: false,
        },
    ] {
        assert!(matches!(host.authorize(operation), Err(FsError::Denied)));
    }
    let dir = tempfile::tempdir().unwrap();
    let alias = dir.path().join("alias");
    std::os::unix::fs::symlink(file, &alias).unwrap();
    for denied in [
        alias.as_path(),
        Path::new("/usr/share/zoneinfo/../zoneinfo/UTC"),
        Path::new("/etc/passwd"),
    ] {
        assert!(
            matches!(host.read_file(denied), Err(FsError::Denied)),
            "{denied:?}"
        );
    }
}

#[test]
fn ambient_temp_dir_uses_native_path() {
    let host = host_fs(None, Arc::new(Env::ambient()));

    #[cfg(windows)]
    assert_eq!(
        host.temp_dir().unwrap(),
        super::super::os::path::temp_dir().unwrap()
    );
    #[cfg(unix)]
    {
        let environment =
            Env::owned(std::env::var_os("TMPDIR").map(|value| ("TMPDIR".into(), value))).unwrap();
        assert_eq!(
            host.temp_dir().unwrap(),
            super::temp_dir::resolve(&environment).unwrap()
        );
    }
}

#[cfg(unix)]
#[test]
fn owned_temp_dir_uses_runtime_environment() {
    use std::os::unix::ffi::OsStrExt;

    let environment = Arc::new(Env::owned([("TMPDIR".into(), "/runtime/tmp".into())]).unwrap());
    let host = host_fs(None, environment);

    assert_eq!(host.temp_dir().unwrap().as_bytes(), b"/runtime/tmp/");
}

#[cfg(unix)]
#[test]
fn temp_dir_reflects_runtime_environment_changes() {
    use std::os::unix::ffi::OsStrExt;

    let environment = Arc::new(Env::owned([("TMPDIR".into(), "/first".into())]).unwrap());
    let host = host_fs(None, Arc::clone(&environment));

    assert_eq!(host.temp_dir().unwrap().as_bytes(), b"/first/");
    environment.set("TMPDIR".into(), "/second".into()).unwrap();
    assert_eq!(host.temp_dir().unwrap().as_bytes(), b"/second/");
    environment.unset("TMPDIR".as_ref()).unwrap();
    assert_eq!(
        host.temp_dir().unwrap(),
        super::temp_dir::resolve(&Env::owned([]).unwrap()).unwrap()
    );
}

#[cfg(all(unix, not(target_os = "android")))]
#[test]
fn empty_owned_environment_uses_default_temp_dir() {
    use std::os::unix::ffi::OsStrExt;

    let environment = Arc::new(Env::owned(Vec::<(OsString, OsString)>::new()).unwrap());
    let host = host_fs(None, environment);

    assert_eq!(host.temp_dir().unwrap().as_bytes(), b"/tmp/");
}

#[cfg(windows)]
#[test]
fn owned_temp_dir_requires_configured_windows_environment() {
    let environment = Arc::new(Env::owned(Vec::<(OsString, OsString)>::new()).unwrap());
    let host = host_fs(None, Arc::clone(&environment));

    assert!(matches!(host.temp_dir(), Err(FsError::Denied)));
    environment.set("TEMP".into(), "C:/Temp".into()).unwrap();
    assert_eq!(host.temp_dir().unwrap().to_string_lossy(), "C:/Temp\\");
    environment.unset("TEMP".as_ref()).unwrap();
    assert!(matches!(host.temp_dir(), Err(FsError::Denied)));
}

#[cfg(windows)]
#[test]
fn empty_tmp_falls_back_to_temp() {
    let environment = Arc::new(
        Env::owned([
            ("TMP".into(), "".into()),
            ("TEMP".into(), "C:/Fallback".into()),
        ])
        .unwrap(),
    );
    let host = host_fs(None, environment);

    assert_eq!(host.temp_dir().unwrap().to_string_lossy(), "C:/Fallback\\");
}

#[test]
fn create_mode_requires_write_even_for_read_access() {
    let tmp = tempfile::tempdir().unwrap();
    let allowed = tmp.path().join("allowed");
    std::fs::create_dir(&allowed).unwrap();
    let policy_file = tmp.path().join("policy.toml");
    std::fs::write(&policy_file, "[fs]\nread = [\"allowed\"]\n").unwrap();
    let (policy, _) = policy::load_file(&policy_file).unwrap();
    let host = host_fs(Some(policy), Arc::new(Env::ambient()));

    assert!(matches!(
        host.authorize(FsOperation::OpenAsync {
            path: FsPath::Cwd(allowed.join("new.txt").as_os_str()),
            access: 0,
            create_mode: 1,
            read_metadata: false,
        }),
        Err(FsError::Denied)
    ));
}

#[test]
fn directory_write_grant_allows_removing_its_root() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let policy_file = tmp.path().join("policy.toml");
    std::fs::write(&policy_file, "[fs]\nwrite = [\"root\"]\n").unwrap();
    let (policy, _) = policy::load_file(&policy_file).unwrap();
    let host = host_fs(Some(policy), Arc::new(Env::ambient()));

    host.remove_directory_all(&root).unwrap();

    assert!(!root.exists());
}

#[test]
fn directory_write_grant_allows_moving_its_root() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("root");
    let destination = tmp.path().join("moved");
    std::fs::create_dir(&root).unwrap();
    let policy_file = tmp.path().join("policy.toml");
    std::fs::write(
        &policy_file,
        "[fs]\nwrite = [\"root\", { glob = \"moved\" }]\n",
    )
    .unwrap();
    let (policy, _) = policy::load_file(&policy_file).unwrap();
    let host = host_fs(Some(policy), Arc::new(Env::ambient()));

    host.rename(&root, &destination).unwrap();

    assert!(!root.exists());
    assert!(destination.is_dir());
}

#[test]
fn wildcard_write_grant_allows_creation_outside_policy_directory() {
    let config = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let policy_file = config.path().join("policy.toml");
    std::fs::write(&policy_file, "[fs]\nwrite = [\"*\"]\n").unwrap();
    let (policy, _) = policy::load_file(&policy_file).unwrap();
    let host = host_fs(Some(policy), Arc::new(Env::ambient()));
    let target = outside.path().join("file");

    host.write_file(&target, b"allowed").unwrap();

    assert_eq!(std::fs::read(target).unwrap(), b"allowed");
}

#[test]
fn relative_at_authorization_uses_the_open_directory_path() {
    let tmp = tempfile::tempdir().unwrap();
    let allowed = tmp.path().join("allowed");
    std::fs::create_dir(&allowed).unwrap();
    std::fs::write(allowed.join("input.txt"), "input").unwrap();
    let policy_file = tmp.path().join("policy.toml");
    std::fs::write(&policy_file, "[fs]\nread = [\"allowed\"]\n").unwrap();
    let (policy, _) = policy::load_file(&policy_file).unwrap();
    let host = host_fs(Some(policy), Arc::new(Env::ambient()));

    host.authorize(FsOperation::Read(FsPath::At {
        base: &allowed,
        path: OsStr::new("input.txt"),
    }))
    .unwrap();
}

#[cfg(unix)]
#[test]
fn unrestricted_relative_lookup_needs_no_directory_path() {
    let directory = tempfile::tempdir().unwrap();
    let file = fs::File::open(directory.path()).unwrap();
    fs::remove_dir(directory.path()).unwrap();
    let host = host_fs(None, Arc::new(Env::ambient()));

    host.authorize(FsOperation::Metadata(FsTarget::Path(FsPath::AtHandle {
        directory: super::super::os::raw_file(&file),
        path: OsStr::new("."),
    })))
    .unwrap();
}

#[cfg(unix)]
#[test]
fn restricted_absolute_lookup_still_checks_its_directory_handle() {
    let directory = tempfile::tempdir().unwrap();
    let file = fs::File::open(directory.path()).unwrap();
    fs::remove_dir(directory.path()).unwrap();
    let environment = Arc::new(Env::ambient());
    let (policy, _) = policy::load_json(br#"{"fs":{"read":["*"]}}"#, &environment).unwrap();
    let host = host_fs(Some(policy), environment);

    let result = host.authorize(FsOperation::Metadata(FsTarget::Path(FsPath::AtHandle {
        directory: super::super::os::raw_file(&file),
        path: OsStr::new("/"),
    })));
    assert!(result.is_err(), "{result:?}");
}

#[cfg(unix)]
#[test]
fn creation_policy_does_not_walk_past_unresolved_symlinks() {
    let tmp = tempfile::tempdir().unwrap();
    let allowed = tmp.path().join("allowed");
    let denied = tmp.path().join("denied");
    std::fs::create_dir(&allowed).unwrap();
    std::fs::create_dir(&denied).unwrap();
    let link = allowed.join("link");
    std::os::unix::fs::symlink("../denied/missing", &link).unwrap();
    let policy_file = tmp.path().join("policy.toml");
    std::fs::write(&policy_file, "[fs]\nwrite = [\"allowed\"]\n").unwrap();
    let (policy, _) = policy::load_file(&policy_file).unwrap();
    let host = host_fs(Some(policy), Arc::new(Env::ambient()));

    assert!(matches!(
        host.write_file(&link, b"escape"),
        Err(FsError::Denied)
    ));
    let directory = Directory::preopen(&allowed).unwrap();
    assert!(matches!(
        host.open_file_at(
            &directory,
            Path::new("link"),
            FileOpenOptions {
                read: false,
                write: true,
                append: false,
                create: true,
                create_new: false,
                truncate: false,
            }
        ),
        Err(FsError::Denied)
    ));
    // Walking past a missing descendant must still inspect its link ancestor.
    assert!(matches!(
        host.create_directory_all(&link.join("child")),
        Err(FsError::Denied)
    ));
    for path in ["link/", "link/.", "new/../link/child"] {
        assert!(matches!(
            host.create_directory_all(&allowed.join(path)),
            Err(FsError::Denied)
        ));
    }
    assert!(!denied.join("missing").exists());
    assert!(!allowed.join("new").exists());

    std::os::unix::fs::symlink("loop", allowed.join("loop")).unwrap();
    assert!(matches!(
        host.authorize(FsOperation::Write(FsPath::Cwd(
            allowed.join("loop").as_os_str()
        ))),
        Err(FsError::Denied)
    ));
    host.unlink_file(&allowed.join("loop")).unwrap();

    host.create_directory_all(&allowed.join("new/child"))
        .unwrap();
    host.write_file(&allowed.join("new/file"), b"allowed")
        .unwrap();
    let renamed = allowed.join("renamed");
    host.rename(&link, &renamed).unwrap();
    assert_eq!(
        std::fs::read_link(&renamed).unwrap(),
        Path::new("../denied/missing")
    );
    host.unlink_file(&renamed).unwrap();
    assert!(std::fs::symlink_metadata(renamed).is_err());
}

#[cfg(unix)]
#[test]
fn unrestricted_creation_keeps_native_dangling_link_behavior() {
    let tmp = tempfile::tempdir().unwrap();
    let link = tmp.path().join("link");
    std::os::unix::fs::symlink("missing", &link).unwrap();
    let host = host_fs(None, Arc::new(Env::ambient()));

    host.write_file(&link, b"native").unwrap();
    assert_eq!(
        std::fs::read(tmp.path().join("missing")).unwrap(),
        b"native"
    );
    assert!(std::fs::symlink_metadata(link).unwrap().is_symlink());
}

#[cfg(unix)]
#[test]
fn directory_suffix_policy_rejects_dangling_links_but_allows_new_entries() {
    let tmp = tempfile::tempdir().unwrap();
    let allowed = tmp.path().join("allowed");
    let denied = tmp.path().join("denied");
    std::fs::create_dir(&allowed).unwrap();
    std::fs::create_dir(&denied).unwrap();
    std::os::unix::fs::symlink("../denied/new", allowed.join("link")).unwrap();
    let policy_file = tmp.path().join("policy.toml");
    std::fs::write(&policy_file, "[fs]\nwrite = [\"allowed\"]\n").unwrap();
    let (policy, _) = policy::load_file(&policy_file).unwrap();
    let host = host_fs(Some(policy), Arc::new(Env::ambient()));
    let link = allowed.join("link/");
    assert!(matches!(host.create_directory(&link), Err(FsError::Denied)));
    let source = allowed.join("source");
    std::fs::create_dir(&source).unwrap();
    assert!(matches!(host.rename(&source, &link), Err(FsError::Denied)));
    assert!(source.is_dir());
    assert!(!denied.join("new").exists());

    host.create_directory(&allowed.join("new/")).unwrap();
    host.rename(&source, &allowed.join("new/")).unwrap();
    assert!(!source.exists());
    assert!(allowed.join("new").is_dir());
}

#[cfg(unix)]
#[test]
fn remove_policy_checks_link_path_not_target() {
    let tmp = tempfile::tempdir().unwrap();
    let allowed = tmp.path().join("allowed");
    let denied = tmp.path().join("denied");
    std::fs::create_dir(&allowed).unwrap();
    std::fs::create_dir(&denied).unwrap();
    let allowed_file = allowed.join("target.txt");
    let denied_link = denied.join("link.txt");
    std::fs::write(&allowed_file, "target").unwrap();
    std::os::unix::fs::symlink(&allowed_file, &denied_link).unwrap();
    let policy_file = tmp.path().join("policy.toml");
    std::fs::write(&policy_file, "[fs]\nwrite = [\"allowed\"]\n").unwrap();
    let (policy, _) = policy::load_file(&policy_file).unwrap();
    let host = host_fs(Some(policy), Arc::new(Env::ambient()));

    assert!(matches!(
        host.authorize(FsOperation::ChangeEntry(FsPath::Cwd(
            denied_link.as_os_str()
        ))),
        Err(FsError::Denied)
    ));
}

#[cfg(unix)]
#[test]
fn remove_policy_checks_link_entry_not_target() {
    let tmp = tempfile::tempdir().unwrap();
    let allowed = tmp.path().join("allowed");
    let denied = tmp.path().join("denied");
    std::fs::create_dir(&allowed).unwrap();
    std::fs::create_dir(&denied).unwrap();
    let denied_file = denied.join("target.txt");
    let allowed_link = allowed.join("link.txt");
    std::fs::write(&denied_file, "target").unwrap();
    std::os::unix::fs::symlink(&denied_file, &allowed_link).unwrap();
    let policy_file = tmp.path().join("policy.toml");
    std::fs::write(&policy_file, "[fs]\nwrite = [\"allowed\"]\n").unwrap();
    let (policy, _) = policy::load_file(&policy_file).unwrap();
    let host = host_fs(Some(policy), Arc::new(Env::ambient()));

    host.authorize(FsOperation::ChangeEntry(FsPath::Cwd(
        allowed_link.as_os_str(),
    )))
    .unwrap();
    assert!(matches!(
        host.authorize(FsOperation::Write(FsPath::Cwd(allowed_link.as_os_str()))),
        Err(FsError::Denied)
    ));
}
