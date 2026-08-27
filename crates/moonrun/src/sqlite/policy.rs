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

use std::ffi::{CStr, c_char, c_int, c_void};
use std::path::Path;
use std::ptr::{self, NonNull};

use libsqlite3_sys as ffi;

use crate::filesystem::HostFs;
use crate::runtime::null_handle;

/// Ensure the requested database and VFS are available in the MVP.
///
/// File-backed connections use SQLite's default VFS. The main database is
/// authorized for reading together with its parent directory because SQLite
/// may read journal, WAL, and shared-memory files beside it. Writable
/// connections also require write access to that directory. This is an
/// admission check, not VFS mediation: SQLite still interprets the filename
/// again when `sqlite3_open_v2` runs.
pub(super) fn ensure_valid_database(
    filesystem: &HostFs,
    filename: &CStr,
    flags: i32,
    vfs: u64,
) -> Result<(), i32> {
    if vfs != null_handle() {
        return Err(ffi::SQLITE_CANTOPEN);
    }
    if filename.to_bytes() == b":memory:" {
        return Ok(());
    }

    let filename = filename.to_str().map_err(|_| ffi::SQLITE_CANTOPEN)?;
    if filename.is_empty() {
        return Err(ffi::SQLITE_CANTOPEN);
    }
    let database = Path::new(filename);
    filesystem
        .authorize_read(database.as_os_str())
        .map_err(|_| ffi::SQLITE_CANTOPEN)?;
    let parent = database
        .parent()
        .filter(|path| !path.as_os_str().is_empty());
    let parent = parent.unwrap_or_else(|| Path::new("."));
    filesystem
        .authorize_read(parent.as_os_str())
        .map_err(|_| ffi::SQLITE_CANTOPEN)?;

    if flags & (ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE) != 0 {
        filesystem
            .authorize_write(parent.as_os_str())
            .map_err(|_| ffi::SQLITE_CANTOPEN)?;
    }
    Ok(())
}

/// Preserve SQLite's access-mode bits and remove every extension flag.
pub(super) fn ensure_open_flags(flags: i32) -> i32 {
    flags & (ffi::SQLITE_OPEN_READONLY | ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE)
}

/// Install the complete MVP SQL policy before guest SQL can be prepared.
///
/// The authorizer rejects database attachment and every guest PRAGMA except
/// the encoding selection a caller may perform before schema initialization.
pub(super) fn install_authorizer(database: NonNull<ffi::sqlite3>) -> i32 {
    unsafe {
        ffi::sqlite3_set_authorizer(
            database.as_ptr(),
            Some(untrusted_authorizer),
            ptr::null_mut(),
        )
    }
}

unsafe extern "C" fn untrusted_authorizer(
    _context: *mut c_void,
    action: c_int,
    argument1: *const c_char,
    _argument2: *const c_char,
    _database: *const c_char,
    _trigger: *const c_char,
) -> c_int {
    // SAFETY: SQLite owns the callback arguments and keeps their
    // NUL-terminated strings valid for the duration of this call.
    match action {
        ffi::SQLITE_PRAGMA
            if !argument1.is_null()
                && unsafe { CStr::from_ptr(argument1) }
                    .to_bytes()
                    .eq_ignore_ascii_case(b"encoding") =>
        {
            ffi::SQLITE_OK
        }
        ffi::SQLITE_ATTACH | ffi::SQLITE_DETACH | ffi::SQLITE_PRAGMA => ffi::SQLITE_DENY,
        _ => ffi::SQLITE_OK,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::Policy;
    use std::ffi::CString;

    fn c_path(path: &Path) -> CString {
        CString::new(path.to_str().unwrap()).unwrap()
    }

    fn ensure_valid_ambient_database(
        policy: &crate::policy::Policy,
        filename: &CStr,
        flags: i32,
        vfs: u64,
    ) -> Result<(), i32> {
        ensure_valid_database(
            &crate::sqlite::tests::ambient_filesystem(policy.clone()),
            filename,
            flags,
            vfs,
        )
    }

    #[test]
    fn open_flags_preserve_only_access_modes() {
        for flags in [
            ffi::SQLITE_OPEN_READONLY,
            ffi::SQLITE_OPEN_READWRITE,
            ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE,
            ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE | 0x4000_0000,
        ] {
            assert_eq!(
                ensure_open_flags(flags),
                flags
                    & (ffi::SQLITE_OPEN_READONLY
                        | ffi::SQLITE_OPEN_READWRITE
                        | ffi::SQLITE_OPEN_CREATE)
            );
        }
    }

    #[test]
    fn database_policy_accepts_memory_and_files_with_the_default_vfs() {
        let policy = Policy::allow_all();
        assert_eq!(
            ensure_valid_ambient_database(
                &policy,
                c":memory:",
                ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE,
                null_handle()
            ),
            Ok(())
        );
        assert_eq!(
            ensure_valid_ambient_database(
                &policy,
                c"database.sqlite",
                ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE,
                null_handle()
            ),
            Ok(())
        );
        assert_eq!(
            ensure_valid_ambient_database(
                &policy,
                c":memory:",
                ffi::SQLITE_OPEN_READWRITE,
                u64::MAX
            ),
            Err(ffi::SQLITE_CANTOPEN)
        );
        assert_eq!(
            ensure_valid_ambient_database(&policy, c"", ffi::SQLITE_OPEN_READWRITE, null_handle()),
            Err(ffi::SQLITE_CANTOPEN)
        );
    }

    #[test]
    fn database_policy_requires_read_access_and_parent_write_access() {
        let temp = tempfile::tempdir().unwrap();
        let allowed = temp.path().join("allowed");
        let denied = temp.path().join("denied");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&denied).unwrap();
        let policy_file = temp.path().join("policy.toml");
        std::fs::write(
            &policy_file,
            "[fs]\nread = [\"allowed\"]\nwrite = [\"allowed\"]\n",
        )
        .unwrap();
        let policy = Policy::from_file(&policy_file).unwrap();
        let allowed_database = c_path(&allowed.join("database.sqlite"));
        let denied_database = c_path(&denied.join("database.sqlite"));

        assert_eq!(
            ensure_valid_ambient_database(
                &policy,
                &allowed_database,
                ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE,
                null_handle()
            ),
            Ok(())
        );
        assert_eq!(
            ensure_valid_ambient_database(
                &policy,
                &denied_database,
                ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE,
                null_handle()
            ),
            Err(ffi::SQLITE_CANTOPEN)
        );
    }

    #[test]
    fn database_requires_its_directory_not_just_the_file() {
        let temp = tempfile::tempdir().unwrap();
        let database = temp.path().join("database.sqlite");
        std::fs::write(&database, []).unwrap();
        let policy_file = temp.path().join("policy.toml");
        std::fs::write(
            &policy_file,
            "[fs]\nread = [\"database.sqlite\"]\nwrite = [\"database.sqlite\"]\n",
        )
        .unwrap();
        let policy = Policy::from_file(&policy_file).unwrap();
        let database = c_path(&database);

        assert_eq!(
            ensure_valid_ambient_database(
                &policy,
                &database,
                ffi::SQLITE_OPEN_READONLY,
                null_handle()
            ),
            Err(ffi::SQLITE_CANTOPEN)
        );
        assert_eq!(
            ensure_valid_ambient_database(
                &policy,
                &database,
                ffi::SQLITE_OPEN_READWRITE,
                null_handle()
            ),
            Err(ffi::SQLITE_CANTOPEN)
        );
    }
}
