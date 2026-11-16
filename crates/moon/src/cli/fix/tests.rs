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
use driver::FileChange;
use moonutil::user_log::UserLog;

#[cfg(windows)]
#[test]
fn display_path_relativizes_plain_and_verbatim_paths() {
    for path in [r"C:\p\a.mbt", r"\\?\C:\p\a.mbt"] {
        for base in [r"C:\p", r"\\?\C:\p"] {
            assert_eq!(display_path(Path::new(path), Path::new(base)), "a.mbt");
        }
    }
}

fn file_change(path: &Path, original: &str, fixed: &str) -> FixPlan {
    FixPlan {
        changes: BTreeMap::from([(
            path.to_owned(),
            FileChange {
                original: original.to_owned(),
                fixed: fixed.to_owned(),
                fixes: vec![],
            },
        )]),
        skipped: vec![],
    }
}

#[test]
fn write_leaves_changed_source_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.mbt");
    std::fs::write(&path, "edited meanwhile").unwrap();
    let (log, _) = UserLog::captured(log::LevelFilter::Warn);
    let outcome = write_changes(&file_change(&path, "original", "fixed"), &log);
    assert!(outcome.failed);
    assert!(outcome.written.is_empty());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "edited meanwhile");
}

#[cfg(unix)]
#[test]
fn write_preserves_permissions_and_symlinks() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.mbt");
    let link = dir.path().join("link.mbt");
    std::fs::write(&path, "original").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    symlink(&path, &link).unwrap();
    let (log, _) = UserLog::captured(log::LevelFilter::Warn);
    assert_eq!(
        write_changes(&file_change(&link, "original", "fixed"), &log).written,
        BTreeSet::from([link.clone()])
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "fixed");
    assert_eq!(std::fs::read_link(link).unwrap(), path);
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

#[cfg(unix)]
#[test]
fn failed_write_preserves_original_source() {
    const CHILD_DIR: &str = "MOON_FIX_WRITE_FAILURE_DIR";
    let original = "a".repeat(128);
    let fixed = "b".repeat(256);
    if let Some(dir) = std::env::var_os(CHILD_DIR) {
        // Restrict only this child process. The old direct write truncates
        // the source, writes 64 bytes, then fails; a staged write must leave
        // the original intact and must not report the file as fixed.
        let limit = libc::rlimit {
            rlim_cur: 64,
            rlim_max: 64,
        };
        unsafe {
            assert_ne!(libc::signal(libc::SIGXFSZ, libc::SIG_IGN), libc::SIG_ERR);
            assert_eq!(libc::setrlimit(libc::RLIMIT_FSIZE, &limit), 0);
        }
        let path = Path::new(&dir).join("a.mbt");
        let (log, capture) = UserLog::captured(log::LevelFilter::Warn);
        let outcome = write_changes(&file_change(&path, &original, &fixed), &log);
        assert!(outcome.failed);
        assert!(outcome.written.is_empty());
        assert!(
            capture
                .take()
                .iter()
                .any(|entry| entry.message.contains("failed to fix"))
        );
        assert_eq!(std::fs::read_to_string(path).unwrap(), original);
        assert_eq!(std::fs::read_dir(dir).unwrap().count(), 1);
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.mbt"), original).unwrap();
    snapbox::cmd::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "cli::fix::tests::failed_write_preserves_original_source",
            "--nocapture",
        ])
        .env(CHILD_DIR, dir.path())
        .assert()
        .success();
}
