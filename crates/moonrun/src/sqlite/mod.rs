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

//! SQLite behavior, lifetime state, policy, and runtime adapters.

pub(crate) mod v8;

mod connection;
mod policy;
mod statement;

use std::cell::RefCell;
use std::rc::Rc;

use libsqlite3_sys as ffi;
use slotmap::SecondaryMap;

use crate::host::{HostKey, HostKeys};

use connection::Database;
use statement::Statement;

#[cfg(not(target_endian = "little"))]
compile_error!("moonrun SQLite imports require a little-endian host");

/// Failures of the Host interface itself, distinct from SQLite result codes.
///
/// SQLite result codes are returned inside successful Host calls. These errors
/// mean the caller violated the portable Host contract and become wasm traps
/// in a runtime adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SqliteHostError {
    InvalidHandle,
    InvalidInput,
    Overflow,
}

pub(crate) type SqliteHostResult<T> = Result<T, SqliteHostError>;

/// Per-run SQLite policy, operations, identity, and payload storage.
///
/// Runtime adapters lower their own memory and scalar representations before
/// crossing this interface. The shared Host Key table supplies identity, while
/// SQLite-owned pointers remain private payloads of this module.
pub(crate) struct SqliteHost {
    keys: Rc<RefCell<HostKeys>>,
    databases: RefCell<SecondaryMap<HostKey, Database>>,
    statements: RefCell<SecondaryMap<HostKey, Statement>>,
}

impl SqliteHost {
    pub(crate) fn with_keys(keys: Rc<RefCell<HostKeys>>) -> Self {
        Self {
            keys,
            databases: RefCell::new(SecondaryMap::new()),
            statements: RefCell::new(SecondaryMap::new()),
        }
    }

    /// Describe live SQLite payloads without inspecting another domain's keys.
    pub(crate) fn leak_summary(&self) -> Option<String> {
        let databases = self.databases.borrow().len();
        let statements = self.statements.borrow().len();
        let mut leaks = Vec::new();
        if databases != 0 {
            leaks.push(format!("databases={databases}"));
        }
        if statements != 0 {
            leaks.push(format!("statements={statements}"));
        }
        (!leaks.is_empty()).then(|| leaks.join(", "))
    }
}

impl Drop for SqliteHost {
    fn drop(&mut self) {
        // Statements hold their connections open, so destroy them first.
        for statement in self.statements.get_mut().values() {
            unsafe { ffi::sqlite3_finalize(statement.pointer.as_ptr()) };
        }
        for database in self.databases.get_mut().values() {
            unsafe { ffi::sqlite3_close(database.pointer().as_ptr()) };
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub(super) fn host() -> SqliteHost {
        SqliteHost::with_keys(Rc::new(RefCell::new(HostKeys::default())))
    }

    pub(super) fn open_memory(host: &SqliteHost) -> u64 {
        let outcome = host.open_v2(
            c":memory:",
            ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE,
            crate::host::null_handle(),
        );
        assert_eq!(outcome.code, ffi::SQLITE_OK);
        outcome.database.unwrap()
    }

    pub(super) fn utf16le(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    #[test]
    fn encoding_is_the_only_guest_pragma() {
        let host = host();
        let database = open_memory(&host);
        let encoding = utf16le("PRAGMA encoding='UTF-16le'");
        let outcome = host.prepare16_v2(database, &encoding).unwrap();
        assert_eq!(outcome.code, ffi::SQLITE_OK);
        let statement = outcome.statement.unwrap();
        assert_eq!(host.step(statement), Ok(ffi::SQLITE_DONE));
        assert_eq!(host.finalize(statement), Ok(ffi::SQLITE_OK));

        let temp_store = utf16le("PRAGMA temp_store=MEMORY");
        let outcome = host.prepare16_v2(database, &temp_store).unwrap();
        assert_eq!(outcome.code, ffi::SQLITE_AUTH);
        assert_eq!(outcome.statement, None);
        assert_eq!(host.close(database), Ok(ffi::SQLITE_OK));
    }

    #[test]
    fn guest_pragmas_cannot_change_process_global_configuration() {
        let host = host();
        let database = open_memory(&host);
        let sql = utf16le("PRAGMA hard_heap_limit=1");
        let outcome = host.prepare16_v2(database, &sql).unwrap();

        assert_eq!(outcome.code, ffi::SQLITE_AUTH);
        assert_eq!(outcome.statement, None);
        assert_eq!(host.close(database), Ok(ffi::SQLITE_OK));
    }
}
