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

use std::ptr::NonNull;

use libsqlite3_sys as ffi;
use slotmap::Key;

use super::connection::Database;
use super::{SqliteHost, SqliteHostError, SqliteHostResult};
use crate::runtime::{HostKey, HostResourceKind, null_handle};

pub(super) struct DatabaseMutex {
    database_key: HostKey,
    /// Recursive entries made through this Host that have not been left.
    ///
    /// This is not SQLite's complete mutex ownership state: SQLite may enter
    /// the same mutex internally. It protects the Database lifetime from
    /// unbalanced guest calls by rejecting unmatched leaves and close, and by
    /// releasing outstanding guest entries before Runtime teardown.
    mutex_depth: u64,
}

impl SqliteHost {
    /// Return a stable Handle for the SQLite-owned mutex of `database`.
    ///
    /// The payload retains the owning Database key rather than SQLite's raw
    /// mutex pointer, so every operation revalidates the connection lifetime.
    pub(crate) fn db_mutex(&self, database: u64) -> SqliteHostResult<u64> {
        let database_key = self.database_key(database)?;
        let database = self.database(database)?;
        if !database.is_ready() {
            return Err(SqliteHostError::InvalidInput);
        }
        if unsafe { ffi::sqlite3_db_mutex(database.pointer().as_ptr()) }.is_null() {
            return Ok(null_handle());
        }
        if let Some(mutex) = database.mutex() {
            return Ok(mutex.data().as_ffi());
        }

        let mutex = self
            .keys
            .borrow_mut()
            .insert(HostResourceKind::SqliteDatabaseMutex);
        let replaced = self.mutexes.borrow_mut().insert(
            mutex,
            DatabaseMutex {
                database_key,
                mutex_depth: 0,
            },
        );
        debug_assert!(replaced.is_none());
        self.databases
            .borrow_mut()
            .get_mut(database_key)
            .expect("validated database disappeared")
            .set_mutex(mutex);
        Ok(mutex.data().as_ffi())
    }

    pub(crate) fn mutex_enter(&self, mutex: u64) -> SqliteHostResult<()> {
        self.update_mutex_depth(mutex, |pointer, mutex_depth| {
            let mutex_depth = mutex_depth
                .checked_add(1)
                .ok_or(SqliteHostError::Overflow)?;
            unsafe { ffi::sqlite3_mutex_enter(pointer.as_ptr()) };
            Ok(mutex_depth)
        })
    }

    pub(crate) fn mutex_leave(&self, mutex: u64) -> SqliteHostResult<()> {
        self.update_mutex_depth(mutex, |pointer, mutex_depth| {
            let mutex_depth = mutex_depth
                .checked_sub(1)
                .ok_or(SqliteHostError::InvalidInput)?;
            unsafe { ffi::sqlite3_mutex_leave(pointer.as_ptr()) };
            Ok(mutex_depth)
        })
    }

    pub(super) fn database_mutex_is_entered(&self, database: Database) -> bool {
        database.mutex().is_some_and(|mutex| {
            self.mutexes
                .borrow()
                .get(mutex)
                .is_some_and(|mutex| mutex.mutex_depth != 0)
        })
    }

    pub(super) fn remove_database_mutex(&self, database: Database) {
        let Some(mutex) = database.mutex() else {
            return;
        };
        let removed = self.mutexes.borrow_mut().remove(mutex);
        debug_assert!(removed.is_some());
        let removed = self.keys.borrow_mut().remove(mutex);
        debug_assert_eq!(removed, Some(HostResourceKind::SqliteDatabaseMutex));
    }

    pub(super) fn release_entered_mutexes(&mut self) {
        // SqliteHost contains the Runtime's Rc-backed Host Keys and therefore
        // cannot move across threads; SQLite requires leave on the enter thread.
        for mutex in self.mutexes.get_mut().values_mut() {
            let database = *self
                .databases
                .get_mut()
                .get(mutex.database_key)
                .expect("live mutex lost its database");
            let pointer =
                NonNull::new(unsafe { ffi::sqlite3_db_mutex(database.pointer().as_ptr()) })
                    .expect("live database lost its mutex");
            for _ in 0..mutex.mutex_depth {
                unsafe { ffi::sqlite3_mutex_leave(pointer.as_ptr()) };
            }
            mutex.mutex_depth = 0;
        }
    }

    fn update_mutex_depth(
        &self,
        mutex: u64,
        operation: impl FnOnce(NonNull<ffi::sqlite3_mutex>, u64) -> SqliteHostResult<u64>,
    ) -> SqliteHostResult<()> {
        if mutex == null_handle() {
            return Ok(());
        }
        let mutex_key = self.mutex_key(mutex)?;
        let (database_key, mutex_depth) = self
            .mutexes
            .borrow()
            .get(mutex_key)
            .map(|mutex| (mutex.database_key, mutex.mutex_depth))
            .ok_or(SqliteHostError::InvalidHandle)?;
        let pointer = self.native_mutex(database_key)?;
        let mutex_depth = operation(pointer, mutex_depth)?;
        self.mutexes
            .borrow_mut()
            .get_mut(mutex_key)
            .expect("validated mutex disappeared")
            .mutex_depth = mutex_depth;
        Ok(())
    }

    fn mutex_key(&self, handle: u64) -> SqliteHostResult<HostKey> {
        self.keys
            .borrow()
            .key(handle, HostResourceKind::SqliteDatabaseMutex)
            .ok_or(SqliteHostError::InvalidHandle)
    }

    fn native_mutex(&self, database_key: HostKey) -> SqliteHostResult<NonNull<ffi::sqlite3_mutex>> {
        let database = self
            .databases
            .borrow()
            .get(database_key)
            .copied()
            .ok_or(SqliteHostError::InvalidHandle)?;
        NonNull::new(unsafe { ffi::sqlite3_db_mutex(database.pointer().as_ptr()) })
            .ok_or(SqliteHostError::InvalidHandle)
    }
}
