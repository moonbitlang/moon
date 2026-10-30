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

//! Policy tests use the same Runtime construction and WASI operations as adapters.

use super::*;
use crate::runtime::{Executable, RuntimeConfig, WorkingDirectory};

#[test]
fn cross_device_native_error_is_preserved_as_xdev() {
    #[cfg(unix)]
    let code = libc::EXDEV;
    #[cfg(windows)]
    let code = windows_sys::Win32::Foundation::ERROR_NOT_SAME_DEVICE as i32;
    assert_eq!(
        path_error_to_errno(crate::filesystem::PathError::Io(
            std::io::Error::from_raw_os_error(code)
        )),
        WASI_ERRNO_XDEV,
    );
}

#[cfg(unix)]
#[test]
fn rename_across_filesystems_does_not_copy_or_remove_the_source() {
    use std::os::unix::fs::MetadataExt;

    let source_dir = tempfile::tempdir().unwrap();
    // Linux CI supplies tmpfs here. A runner without a second writable
    // filesystem can still exercise the portable native-error mapping test.
    let Ok(target_dir) = tempfile::tempdir_in("/dev/shm") else {
        eprintln!("no writable /dev/shm for the cross-filesystem rename control");
        return;
    };
    if fs::metadata(source_dir.path()).unwrap().dev()
        == fs::metadata(target_dir.path()).unwrap().dev()
    {
        eprintln!("temporary directories share a filesystem; cross-filesystem control unavailable");
        return;
    }
    let source = source_dir.path().join("source");
    let target = target_dir.path().join("target");
    fs::write(&source, b"keep").unwrap();
    assert_eq!(
        fs::rename(&source, &target).unwrap_err().raw_os_error(),
        Some(libc::EXDEV)
    );
    let context = tests::test_context(Path::new("/"));
    let source_bytes = source.as_os_str().as_encoded_bytes();
    let target_bytes = target.as_os_str().as_encoded_bytes();
    let mut memory = [source_bytes, target_bytes].concat();
    assert_eq!(
        path_rename_impl(
            &context,
            &mut memory,
            3,
            0,
            source_bytes.len() as u32,
            3,
            source_bytes.len() as u32,
            target_bytes.len() as u32,
        ),
        Err(WASI_ERRNO_XDEV),
    );
    assert_eq!(fs::read(source).unwrap(), b"keep");
    assert!(!target.exists());
}

fn context(root: &Path, policy: &str) -> WasiContext {
    let policy_file = root.join("policy.toml");
    fs::write(&policy_file, policy).unwrap();
    let runtime = Runtime::new(
        RuntimeConfig {
            policy_file: Some(policy_file),
            working_directory: WorkingDirectory::Fixed(root.to_owned()),
            ..Default::default()
        },
        Executable::unavailable(),
        crate::signal_channel().1,
    )
    .unwrap();
    WasiContext::new(
        "authority.wasm",
        &[],
        &runtime,
        TerminationRequest::default(),
    )
}

fn open(
    context: &WasiContext,
    dirfd: i32,
    path: &str,
    oflags: i32,
    rights: u64,
    inheriting: u64,
    flags: i32,
) -> WasiResult<i32> {
    let mut memory = vec![0; 1024];
    memory[..path.len()].copy_from_slice(path.as_bytes());
    path_open_impl(
        context,
        &mut memory,
        dirfd,
        WASI_LOOKUPFLAG_SYMLINK_FOLLOW,
        0,
        path.len() as u32,
        oflags,
        rights,
        inheriting,
        flags,
        512,
    )?;
    Ok(i32::from_le_bytes(memory[512..516].try_into().unwrap()))
}

#[test]
fn denied_open_neither_acquires_a_descriptor_nor_changes_the_file() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("file"), b"keep").unwrap();
    let context = context(root.path(), "");
    for (path, oflags, rights, flags) in [
        ("file", 0, WASI_RIGHT_FD_READ, 0),
        ("file", 0, WASI_RIGHT_FD_WRITE, 0),
        ("file", 0, 0, 0), // The native open still acquires read access.
        ("file", WASI_OFLAGS_TRUNC, WASI_RIGHT_FD_WRITE, 0),
        ("file", 0, WASI_RIGHT_FD_READ, WASI_FDFLAG_APPEND),
        ("new", WASI_OFLAGS_CREAT, WASI_RIGHT_FD_READ, 0),
    ] {
        assert_eq!(
            open(&context, 3, path, oflags, rights, 0, flags),
            Err(WASI_ERRNO_ACCESS),
            "{path}, flags={oflags}/{flags}, rights={rights}"
        );
        assert!(context.descriptors.lock().unwrap().entries.is_empty());
        assert_eq!(fs::read(root.path().join("file")).unwrap(), b"keep");
        assert!(!root.path().join("new").exists());
    }
}

#[test]
fn file_grants_and_descriptor_rights_are_both_required() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("file"), b"keep").unwrap();
    let context = context(root.path(), "[fs]\nread = [\".\"]\n");
    for (oflags, rights, flags) in [
        (0, WASI_RIGHT_FD_WRITE, 0),
        (WASI_OFLAGS_TRUNC, WASI_RIGHT_FD_WRITE, 0),
        (WASI_OFLAGS_TRUNC, WASI_RIGHT_FD_READ, 0),
        (WASI_OFLAGS_CREAT, WASI_RIGHT_FD_READ, 0),
        (0, WASI_RIGHT_FD_READ, WASI_FDFLAG_APPEND),
    ] {
        assert_eq!(
            open(&context, 3, "file", oflags, rights, 0, flags),
            Err(WASI_ERRNO_ACCESS)
        );
        assert_eq!(fs::read(root.path().join("file")).unwrap(), b"keep");
    }
    let fd = open(&context, 3, "file", 0, WASI_RIGHT_FD_READ, 0, 0).unwrap();
    let mut memory = [0; 32];
    memory[..4].copy_from_slice(&16u32.to_le_bytes());
    memory[4..8].copy_from_slice(&4u32.to_le_bytes());
    assert_eq!(
        fd_write_impl(&context, &mut memory, fd, 0, 1, 8),
        Err(WASI_ERRNO_NOTCAPABLE)
    );
    fd_read_impl(&context, &mut memory, fd, 0, 1, 8).unwrap();
    assert_eq!(&memory[16..20], b"keep");
}

#[test]
fn write_only_grant_does_not_authorize_the_native_read_fallback() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("file"), b"keep").unwrap();
    let context = context(root.path(), "[fs]\nwrite = [\".\"]\n");
    assert_eq!(
        open(&context, 3, "file", 0, 0, 0, 0),
        Err(WASI_ERRNO_ACCESS)
    );
    assert_eq!(
        open(
            &context,
            3,
            "file",
            0,
            WASI_RIGHT_FD_READ | WASI_RIGHT_FD_WRITE,
            0,
            0
        ),
        Err(WASI_ERRNO_ACCESS)
    );
    let fd = open(&context, 3, "file", 0, WASI_RIGHT_FD_WRITE, 0, 0).unwrap();
    let mut memory = [0; 32];
    memory[..4].copy_from_slice(&16u32.to_le_bytes());
    memory[4..8].copy_from_slice(&1u32.to_le_bytes());
    memory[16] = b'K';
    fd_write_impl(&context, &mut memory, fd, 0, 1, 8).unwrap();
    assert_eq!(
        fd_read_impl(&context, &mut memory, fd, 0, 1, 8),
        Err(WASI_ERRNO_NOTCAPABLE)
    );
    assert_eq!(fs::read(root.path().join("file")).unwrap(), b"Keep");
}

#[test]
fn entry_mutations_require_write_grants_before_effects() {
    for policy in ["", "[fs]\nread = [\".\"]\n", "[fs]\nwrite = [\".\"]\n"] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("file"), b"keep").unwrap();
        fs::create_dir(root.path().join("empty")).unwrap();
        let context = context(root.path(), policy);
        let mut memory = [0; 64];
        memory[..12].copy_from_slice(b"newfileempty");
        let expected = if policy.contains("write") {
            Ok(())
        } else {
            Err(WASI_ERRNO_ACCESS)
        };
        assert_eq!(
            path_create_directory_impl(&context, &mut memory, 3, 0, 3),
            expected
        );
        assert_eq!(
            path_unlink_file_impl(&context, &mut memory, 3, 3, 4),
            expected
        );
        assert_eq!(
            path_remove_directory_impl(&context, &mut memory, 3, 7, 5),
            expected
        );
        assert_eq!(root.path().join("new").exists(), expected.is_ok());
        assert_eq!(root.path().join("file").exists(), expected.is_err());
        assert_eq!(root.path().join("empty").exists(), expected.is_err());
    }
}

#[test]
fn rename_authorizes_both_entries() {
    for (source, target, permitted) in [
        ("allowed/a", "denied/b", false),
        ("denied/a", "allowed/b", false),
        ("allowed/a", "allowed/b", true),
    ] {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("allowed")).unwrap();
        fs::create_dir(root.path().join("denied")).unwrap();
        fs::write(root.path().join(source), b"source").unwrap();
        fs::write(root.path().join(target), b"target").unwrap();
        let context = context(root.path(), "[fs]\nwrite = [\"allowed\"]\n");
        let mut memory = [0; 128];
        memory[..source.len()].copy_from_slice(source.as_bytes());
        memory[64..64 + target.len()].copy_from_slice(target.as_bytes());
        assert_eq!(
            path_rename_impl(
                &context,
                &mut memory,
                3,
                0,
                source.len() as u32,
                3,
                64,
                target.len() as u32
            ),
            if permitted {
                Ok(())
            } else {
                Err(WASI_ERRNO_ACCESS)
            }
        );
        assert_eq!(
            fs::read(root.path().join(target)).unwrap(),
            if permitted { b"source" } else { b"target" }
        );
        assert_eq!(root.path().join(source).exists(), !permitted);
    }
}

#[test]
fn directory_enumeration_and_open_metadata_require_read_authority() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("allowed")).unwrap();
    fs::write(root.path().join("allowed/file"), b"keep").unwrap();
    let context = context(root.path(), "[fs]\nread = [\"allowed\"]\n");
    let mut memory = [0; 512];
    assert_eq!(
        fd_readdir_impl(&context, &mut memory, 3, 0, 256, 0, 256),
        Err(WASI_ERRNO_ACCESS)
    );
    assert_eq!(
        open(&context, 3, ".", WASI_OFLAGS_DIRECTORY, 0, 0, 0),
        Err(WASI_ERRNO_ACCESS)
    );
    let fd = open(
        &context,
        3,
        "allowed",
        WASI_OFLAGS_DIRECTORY,
        WASI_RIGHT_FD_READDIR | WASI_RIGHT_PATH_OPEN,
        WASI_RIGHT_FD_READ,
        0,
    )
    .unwrap();
    fd_readdir_impl(&context, &mut memory, fd, 0, 256, 0, 256).unwrap();
    assert!(u32::from_le_bytes(memory[256..260].try_into().unwrap()) > 0);
    open(&context, fd, "file", 0, WASI_RIGHT_FD_READ, 0, 0).unwrap();
    assert_eq!(
        open(&context, fd, "file", 0, WASI_RIGHT_FD_WRITE, 0, 0),
        Err(WASI_ERRNO_NOTCAPABLE)
    );
}

#[cfg(unix)]
#[test]
fn symlink_read_and_removal_authorize_the_entry_but_open_authorizes_the_target() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("allowed")).unwrap();
    fs::write(root.path().join("secret"), b"secret").unwrap();
    std::os::unix::fs::symlink("../secret", root.path().join("allowed/link")).unwrap();
    let context = context(
        root.path(),
        "[fs]\nread = [\"allowed\"]\nwrite = [\"allowed\"]\n",
    );
    let mut memory = [0; 128];
    memory[..12].copy_from_slice(b"allowed/link");
    path_readlink_impl(&context, &mut memory, 3, 0, 12, 32, 64, 96).unwrap();
    assert_eq!(&memory[32..41], b"../secret");
    assert_eq!(
        open(&context, 3, "allowed/link", 0, WASI_RIGHT_FD_READ, 0, 0),
        Err(WASI_ERRNO_ACCESS)
    );
    path_unlink_file_impl(&context, &mut memory, 3, 0, 12).unwrap();
    assert_eq!(fs::read(root.path().join("secret")).unwrap(), b"secret");
}

#[cfg(unix)]
#[test]
fn directory_replacement_cannot_reuse_a_previous_path_grant() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("allowed")).unwrap();
    fs::create_dir(root.path().join("denied")).unwrap();
    fs::write(root.path().join("denied/file"), b"secret").unwrap();
    let context = context(root.path(), "[fs]\nread = [\"allowed\"]\n");
    let fd = open(
        &context,
        3,
        "allowed",
        WASI_OFLAGS_DIRECTORY,
        WASI_RIGHT_FD_READDIR | WASI_RIGHT_PATH_OPEN,
        WASI_RIGHT_FD_READ,
        0,
    )
    .unwrap();
    fs::remove_dir(root.path().join("allowed")).unwrap();
    std::os::unix::fs::symlink("denied", root.path().join("allowed")).unwrap();
    assert_eq!(
        open(&context, fd, "file", 0, WASI_RIGHT_FD_READ, 0, 0),
        Err(WASI_ERRNO_ACCESS)
    );
    assert_eq!(
        fd_readdir_impl(&context, &mut [0; 512], fd, 0, 256, 0, 256),
        Err(WASI_ERRNO_ACCESS)
    );
}

#[cfg(unix)]
#[test]
fn acquired_file_access_survives_a_host_rename() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("file"), b"keep").unwrap();
    let context = context(root.path(), "[fs]\nread = [\"file\"]\n");
    let fd = open(&context, 3, "file", 0, WASI_RIGHT_FD_READ, 0, 0).unwrap();
    fs::rename(root.path().join("file"), root.path().join("renamed")).unwrap();
    let mut memory = [0; 32];
    memory[..4].copy_from_slice(&16u32.to_le_bytes());
    memory[4..8].copy_from_slice(&4u32.to_le_bytes());
    fd_read_impl(&context, &mut memory, fd, 0, 1, 8).unwrap();
    assert_eq!(&memory[16..20], b"keep");
    assert_eq!(
        open(&context, 3, "renamed", 0, WASI_RIGHT_FD_READ, 0, 0),
        Err(WASI_ERRNO_ACCESS)
    );
}

#[cfg(unix)]
#[test]
fn readlink_requires_authority_for_the_link_entry() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("file"), b"keep").unwrap();
    std::os::unix::fs::symlink("file", root.path().join("link")).unwrap();
    let context = context(root.path(), "[fs]\nread = [\"file\"]\n");
    let mut memory = [0; 128];
    memory[..4].copy_from_slice(b"link");
    assert_eq!(
        path_readlink_impl(&context, &mut memory, 3, 0, 4, 32, 64, 96),
        Err(WASI_ERRNO_ACCESS)
    );
    open(&context, 3, "link", 0, WASI_RIGHT_FD_READ, 0, 0).unwrap();
}

#[test]
fn filesystem_allow_all_does_not_widen_the_preopen() {
    let outer = tempfile::tempdir().unwrap();
    let root = outer.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(outer.path().join("outside"), b"secret").unwrap();
    let context = context(&root, "[fs]\nread = [\"*\"]\nwrite = [\"*\"]\n");
    assert_eq!(
        open(&context, 3, "../outside", 0, WASI_RIGHT_FD_READ, 0, 0),
        Err(WASI_ERRNO_NOTCAPABLE)
    );
}

#[test]
fn unlink_file_keeps_native_file_only_semantics() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("directory")).unwrap();
    let context = context(root.path(), "[fs]\nwrite = [\".\"]\n");
    let mut memory = *b"directory";
    assert!(path_unlink_file_impl(&context, &mut memory, 3, 0, 9).is_err());
    assert!(root.path().join("directory").is_dir());
    path_remove_directory_impl(&context, &mut memory, 3, 0, 9).unwrap();
}

#[cfg(unix)]
#[test]
fn directory_suffixes_cannot_turn_entry_permissions_into_target_permissions() {
    for target_operand in [false, true] {
        for suffix in ["/", "//", "/."] {
            let root = tempfile::tempdir().unwrap();
            fs::create_dir(root.path().join("allowed")).unwrap();
            fs::create_dir(root.path().join("denied")).unwrap();
            fs::create_dir(root.path().join("allowed/moved")).unwrap();
            std::os::unix::fs::symlink("../denied", root.path().join("allowed/link")).unwrap();
            let context = context(root.path(), "[fs]\nwrite = [\"allowed\"]\n");
            let link = format!("allowed/link{suffix}");
            let (source, target) = if target_operand {
                ("allowed/moved", link.as_str())
            } else {
                (link.as_str(), "allowed/moved")
            };
            let mut memory = [0; 128];
            memory[..source.len()].copy_from_slice(source.as_bytes());
            memory[64..64 + target.len()].copy_from_slice(target.as_bytes());
            assert_eq!(
                path_rename_impl(
                    &context,
                    &mut memory,
                    3,
                    0,
                    source.len() as u32,
                    3,
                    64,
                    target.len() as u32
                ),
                Err(WASI_ERRNO_ACCESS),
                "source={source} target={target}"
            );
            memory[..link.len()].copy_from_slice(link.as_bytes());
            assert_eq!(
                path_remove_directory_impl(&context, &mut memory, 3, 0, link.len() as u32),
                Err(WASI_ERRNO_ACCESS)
            );
            assert!(root.path().join("denied").is_dir());
            assert!(root.path().join("allowed/link").is_symlink());
            assert!(root.path().join("allowed/moved").is_dir());
        }
    }
}

#[cfg(unix)]
#[test]
fn directory_suffixes_cannot_escape_the_preopen_without_a_policy() {
    let outer = tempfile::tempdir().unwrap();
    let root = outer.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::create_dir(outer.path().join("outside")).unwrap();
    std::os::unix::fs::symlink("../outside", root.join("link")).unwrap();
    let context = super::tests::test_context(&root);
    let mut memory = [0; 128];
    memory[..5].copy_from_slice(b"link/");
    memory[64..69].copy_from_slice(b"moved");
    assert_eq!(
        path_rename_impl(&context, &mut memory, 3, 0, 5, 3, 64, 5),
        Err(WASI_ERRNO_NOTCAPABLE)
    );
    assert_eq!(
        path_remove_directory_impl(&context, &mut memory, 3, 0, 5),
        Err(WASI_ERRNO_NOTCAPABLE)
    );
    assert!(outer.path().join("outside").is_dir());
}

#[cfg(unix)]
#[test]
fn missing_directory_suffix_distinguishes_new_entry_from_dangling_link() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("allowed")).unwrap();
    fs::create_dir(root.path().join("denied")).unwrap();
    fs::create_dir(root.path().join("allowed/source")).unwrap();
    std::os::unix::fs::symlink("../denied/new", root.path().join("allowed/link")).unwrap();
    for context in [
        context(root.path(), "[fs]\nwrite = [\"allowed\"]\n"),
        super::tests::test_context(root.path()),
    ] {
        let mut memory = [0; 128];
        memory[..13].copy_from_slice(b"allowed/link/");
        memory[64..78].copy_from_slice(b"allowed/source");
        // WASI already rejects dangling final symlinks; '/' must not evade it.
        assert_eq!(
            path_create_directory_impl(&context, &mut memory, 3, 0, 13),
            Err(WASI_ERRNO_NOTCAPABLE)
        );
        assert_eq!(
            path_rename_impl(&context, &mut memory, 3, 64, 14, 3, 0, 13),
            Err(WASI_ERRNO_NOTCAPABLE)
        );
        assert!(!root.path().join("denied/new").exists());
        assert!(root.path().join("allowed/source").is_dir());
    }
    let context = context(root.path(), "[fs]\nwrite = [\"allowed\"]\n");
    let mut memory = *b"allowed/new/";
    path_create_directory_impl(&context, &mut memory, 3, 0, 12).unwrap();
    assert!(root.path().join("allowed/new").is_dir());
}
