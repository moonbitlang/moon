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

//! SQLite behavior, lifetime state, admission rules, and runtime adapters.

pub(crate) mod wasm;

mod bind;
mod column;
mod connection;
mod jobs;
pub(crate) use jobs::Job;
mod mutex;
mod policy;
mod statement;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use libsqlite3_sys as ffi;

use crate::filesystem::HostFs;
use crate::runtime::{Handles, HostKeys, HostResourceKind};

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

/// Per-run SQLite admission, operations, identity, and payload storage.
///
/// Wasm runtime adapters lower their own memory and scalar representations before
/// crossing this interface. The shared Host Key table supplies identity, while
/// SQLite-owned pointers remain private payloads of this module.
pub(crate) struct SqliteHost {
    filesystem: Arc<HostFs>,
    databases: RefCell<Handles<Database>>,
    mutexes: RefCell<Handles<mutex::DatabaseMutex>>,
    statements: RefCell<Handles<Statement>>,
    job_leases: RefCell<Vec<std::sync::Weak<jobs::Lease>>>,
}

impl SqliteHost {
    pub(crate) fn new(filesystem: Arc<HostFs>, keys: Rc<RefCell<HostKeys>>) -> Self {
        Self {
            filesystem,
            databases: RefCell::new(Handles::new(keys.clone(), HostResourceKind::SqliteDatabase)),
            mutexes: RefCell::new(Handles::new(
                keys.clone(),
                HostResourceKind::SqliteDatabaseMutex,
            )),
            statements: RefCell::new(Handles::new(keys, HostResourceKind::SqliteStatement)),
            job_leases: RefCell::new(Vec::new()),
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
        // A guest may terminate between a balanced enter/leave pair. Release
        // those recursive entries before SQLite destroys connection mutexes.
        self.release_entered_mutexes();
        // Runtime destroys Async Host workers and Jobs before these tables.
        // Statements hold their connections open, so destroy them first.
        for statement in self.statements.get_mut().take_all() {
            unsafe { ffi::sqlite3_finalize(statement.pointer.as_ptr()) };
        }
        self.mutexes.get_mut().take_all();
        for database in self.databases.get_mut().take_all() {
            unsafe { ffi::sqlite3_close(database.pointer().as_ptr()) };
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::policy::Policy;
    use crate::runtime::{Env, WorkingDirectory};

    pub(super) fn ambient_filesystem(policy: Option<Policy>) -> Arc<HostFs> {
        Arc::new(HostFs::new(
            policy.as_ref().map(Policy::filesystem),
            Arc::new(Env::ambient()),
            Arc::new(WorkingDirectory::Ambient),
        ))
    }

    pub(crate) fn runtime() -> crate::runtime::Runtime {
        crate::runtime::Runtime::new(
            crate::runtime::RuntimeConfig::default(),
            crate::runtime::Executable::unavailable(),
            crate::signal_channel().1,
        )
        .unwrap()
    }

    pub(super) fn host() -> SqliteHost {
        SqliteHost::new(
            ambient_filesystem(None),
            Rc::new(RefCell::new(HostKeys::default())),
        )
    }

    pub(super) fn open_memory(host: &SqliteHost) -> u64 {
        let outcome = host.open_v2(
            c":memory:",
            ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE,
            crate::runtime::null_handle(),
        );
        assert_eq!(outcome.code, ffi::SQLITE_OK);
        outcome.database.unwrap()
    }

    pub(super) fn utf16le(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    #[test]
    fn dropping_host_retires_database_statement_and_mutex_handles() {
        use crate::runtime::HostKey;
        use slotmap::KeyData;

        let keys = Rc::new(RefCell::new(HostKeys::default()));
        let host = SqliteHost::new(ambient_filesystem(None), keys.clone());
        let database = open_memory(&host);
        let statement = host
            .prepare16_v2(database, &utf16le("SELECT 1"))
            .unwrap()
            .statement
            .unwrap();
        let mutex = host.db_mutex(database).unwrap();
        assert_ne!(mutex, crate::runtime::null_handle());
        host.mutex_enter(mutex).unwrap();
        host.mutex_enter(mutex).unwrap();

        // Teardown must release abandoned recursive entries, finalize statements
        // before closing their database, and retire all three Handle families.
        drop(host);
        for handle in [database, statement, mutex] {
            assert_eq!(
                keys.borrow().kind(HostKey::from(KeyData::from_ffi(handle))),
                None
            );
        }
    }

    #[test]
    fn busy_close_preserves_handles_until_statements_are_finalized() {
        let host = host();
        let database = open_memory(&host);
        let statement = host
            .prepare16_v2(database, &utf16le("SELECT 1"))
            .unwrap()
            .statement
            .unwrap();
        let mutex = host.db_mutex(database).unwrap();
        assert_eq!(host.close(statement), Err(SqliteHostError::InvalidHandle));
        assert_eq!(host.step(database), Err(SqliteHostError::InvalidHandle));
        assert_eq!(
            host.mutex_enter(statement),
            Err(SqliteHostError::InvalidHandle)
        );

        assert_eq!(host.close(database), Ok(ffi::SQLITE_BUSY));
        assert_eq!(host.db_mutex(database), Ok(mutex));
        assert_eq!(host.step(statement), Ok(ffi::SQLITE_ROW));
        assert_eq!(host.finalize(statement), Ok(ffi::SQLITE_OK));
        assert_eq!(host.close(database), Ok(ffi::SQLITE_OK));

        let replacement = open_memory(&host);
        assert_ne!(replacement, database);
        assert_eq!(host.close(database), Err(SqliteHostError::InvalidHandle));
        assert_eq!(host.step(statement), Err(SqliteHostError::InvalidHandle));
        assert_eq!(host.mutex_enter(mutex), Err(SqliteHostError::InvalidHandle));
        assert_eq!(host.close(replacement), Ok(ffi::SQLITE_OK));
    }

    #[test]
    fn finalize_retires_the_statement_even_when_it_reports_an_execution_error() {
        let host = host();
        let database = open_memory(&host);
        let statement = host
            .prepare16_v2(database, &utf16le("SELECT abs(-9223372036854775808)"))
            .unwrap()
            .statement
            .unwrap();
        assert_eq!(host.step(statement), Ok(ffi::SQLITE_ERROR));
        assert_eq!(host.finalize(statement), Ok(ffi::SQLITE_ERROR));
        assert_eq!(
            host.finalize(statement),
            Err(SqliteHostError::InvalidHandle)
        );
        assert_eq!(host.step(statement), Err(SqliteHostError::InvalidHandle));
        // SQLITE_OK also proves that the native Statement was released.
        assert_eq!(host.close(database), Ok(ffi::SQLITE_OK));
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
