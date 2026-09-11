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

//! SQLite payloads for the existing async Job lifecycle. The Async Host owns
//! scheduling, completion, job Handles, and destruction; this module owns SQLite
//! inputs, captured results, and the leases that keep native pointers valid.
mod runner;

use std::ffi::CString;
use std::sync::{Arc, Weak};

use libsqlite3_sys as ffi;
use slotmap::Key;

use super::statement::Statement;
use super::{SqliteHost, SqliteHostError, SqliteHostResult};
use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::runtime::HostKey;
use runner::{DatabasePointer, Input, Outcome, Output, OwnedStatement, StatementPointer};

/// A lease outlives both execution and unclaimed results, even when free_job
/// detaches an in-flight Job. Weak references in SQLite Host track those actual
/// lifetimes rather than the lifetime of the guest's async Job Handle.
#[derive(Debug)]
pub(super) struct Lease {
    database: Option<HostKey>,
    statement: Option<HostKey>,
}

pub(crate) struct Job {
    input: Option<Input>,
    result: Option<Outcome>,
    // Drop native inputs/results before removing their lifetime protection.
    lease: Arc<Lease>,
}

impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteJob")
            .field("result", &self.result)
            .field("lease", &self.lease)
            .finish_non_exhaustive()
    }
}

impl Job {
    pub(crate) fn run(&mut self) -> AsyncHostResult<i64> {
        let result = self.input.take().ok_or(AsyncHostError::Inval)?.run();
        let code = result.code;
        self.result = Some(result);
        // SQLite statuses belong in ret, not in the pool's OS-error channel.
        Ok(i64::from(code))
    }

    pub(crate) fn copy_result(&self, output: &mut [u8; 24]) -> SqliteHostResult<()> {
        let result = self.result.as_ref().ok_or(SqliteHostError::InvalidInput)?;
        let tail = match result.output {
            Output::Prepare { tail, .. } => tail,
            _ => -1,
        };
        let changes = match result.output {
            Output::Step(changes) => changes,
            _ => None,
        };
        output[0..4].copy_from_slice(&result.code.to_le_bytes());
        output[4..8].copy_from_slice(&result.extended.to_le_bytes());
        output[8..12].copy_from_slice(&tail.to_le_bytes());
        output[12..16].copy_from_slice(&i32::from(changes.is_some()).to_le_bytes());
        output[16..24].copy_from_slice(&changes.unwrap_or(0).to_le_bytes());
        Ok(())
    }

    pub(crate) fn message16_length(&self) -> SqliteHostResult<u32> {
        let result = self.result.as_ref().ok_or(SqliteHostError::InvalidInput)?;
        u32::try_from(result.message.len()).map_err(|_| SqliteHostError::Overflow)
    }

    pub(crate) fn copy_message16(&self, output: &mut [u16]) -> SqliteHostResult<u32> {
        let length = self.message16_length()?;
        let message = &self.result.as_ref().unwrap().message;
        if output.len() >= message.len() {
            output[..message.len()].copy_from_slice(message);
        }
        Ok(length)
    }
}

impl SqliteHost {
    pub(crate) fn make_open_job(&self, filename: CString, flags: i32) -> Job {
        self.make_job(
            None,
            None,
            Input::Open {
                filesystem: Arc::clone(&self.filesystem),
                filename,
                flags,
            },
        )
    }

    pub(crate) fn make_prepare_job(&self, database: u64, sql: Vec<u16>) -> SqliteHostResult<Job> {
        if sql.len() > i32::MAX as usize / 2 {
            return Err(SqliteHostError::Overflow);
        }
        let key = self.database_key(database)?;
        let database = self.database_for_job(database)?;
        Ok(self.make_job(
            Some(key),
            None,
            Input::Prepare {
                database: DatabasePointer(database),
                sql,
            },
        ))
    }

    pub(crate) fn make_step_job(&self, database: u64, statement: u64) -> SqliteHostResult<Job> {
        let database_key = self.database_key(database)?;
        let database = self.database_for_job(database)?;
        let key = self.statement_key(statement)?;
        if self.job_uses_statement(key) {
            return Err(SqliteHostError::InvalidInput);
        }
        let statement = self.statements.borrow()[key];
        if statement.database != database_key {
            return Err(SqliteHostError::InvalidInput);
        }
        Ok(self.make_job(
            Some(database_key),
            Some(key),
            Input::Step {
                database: DatabasePointer(database),
                statement: StatementPointer(statement.pointer),
            },
        ))
    }

    pub(crate) fn make_finalize_job(&self, database: u64, statement: u64) -> SqliteHostResult<Job> {
        let database_key = self.database_key(database)?;
        let database = self.database_for_job(database)?;
        let key = self.statement_key(statement)?;
        if self.job_uses_statement(key) || self.statements.borrow()[key].database != database_key {
            return Err(SqliteHostError::InvalidInput);
        }
        // Validate everything before consuming the Statement. Job construction
        // and pool insertion perform no fallible OS-resource allocation.
        let statement = self.remove_statement(statement)?;
        Ok(self.make_job(
            Some(database_key),
            None,
            Input::Finalize {
                database: DatabasePointer(database),
                statement: OwnedStatement(Some(StatementPointer(statement.pointer))),
            },
        ))
    }

    /// Move an unclaimed result directly into cleanup work without publishing
    /// a Database/Statement Handle. Releasing the source cannot release the
    /// shared lease while the cleanup worker still owns the native pointer.
    pub(crate) fn make_discard_job(&self, job: &mut Job) -> SqliteHostResult<Option<Job>> {
        let result = job.result.as_mut().ok_or(SqliteHostError::InvalidInput)?;
        let input = match &mut result.output {
            Output::Open(database) if database.0.is_some() => {
                Input::Close(runner::OwnedDatabase(database.0.take()))
            }
            Output::Prepare { statement, .. } if statement.0.is_some() => {
                let key = job.lease.database.ok_or(SqliteHostError::InvalidInput)?;
                let database = self.database_for_job(key.data().as_ffi())?;
                Input::Finalize {
                    database: DatabasePointer(database),
                    statement: OwnedStatement(statement.0.take()),
                }
            }
            _ => return Ok(None),
        };
        Ok(Some(Job {
            input: Some(input),
            result: None,
            lease: Arc::clone(&job.lease),
        }))
    }

    pub(crate) fn take_job_handle(&self, job: &mut Job) -> SqliteHostResult<u64> {
        let result = job.result.as_mut().ok_or(SqliteHostError::InvalidInput)?;
        if result.code != ffi::SQLITE_OK {
            return Err(SqliteHostError::InvalidInput);
        }
        match &mut result.output {
            Output::Open(database) => {
                let database = database.0.take().ok_or(SqliteHostError::InvalidInput)?;
                let handle = self.insert_database(database.0);
                Ok(handle)
            }
            Output::Prepare { statement, .. } => {
                let database = job.lease.database.ok_or(SqliteHostError::InvalidInput)?;
                let pointer = statement.0.take().ok_or(SqliteHostError::InvalidInput)?.0;
                Ok(self.insert_statement(Statement { pointer, database }))
            }
            _ => Err(SqliteHostError::InvalidInput),
        }
    }

    pub(super) fn job_uses_database(&self, database: HostKey) -> bool {
        let mut leases = self.job_leases.borrow_mut();
        leases.retain(|lease| lease.strong_count() != 0);
        leases
            .iter()
            .filter_map(Weak::upgrade)
            .any(|lease| lease.database == Some(database))
    }

    pub(super) fn job_uses_statement(&self, statement: HostKey) -> bool {
        let mut leases = self.job_leases.borrow_mut();
        leases.retain(|lease| lease.strong_count() != 0);
        leases
            .iter()
            .filter_map(Weak::upgrade)
            .any(|lease| lease.statement == Some(statement))
    }

    fn database_for_job(&self, database: u64) -> SqliteHostResult<super::connection::Database> {
        let database = self.database(database)?;
        if !database.is_ready() || self.database_mutex_is_entered(database) {
            return Err(SqliteHostError::InvalidInput);
        }
        Ok(database)
    }

    fn make_job(&self, database: Option<HostKey>, statement: Option<HostKey>, input: Input) -> Job {
        let lease = Arc::new(Lease {
            database,
            statement,
        });
        let mut leases = self.job_leases.borrow_mut();
        leases.retain(|lease| lease.strong_count() != 0);
        leases.push(Arc::downgrade(&lease));
        Job {
            input: Some(input),
            result: None,
            lease,
        }
    }
}

#[cfg(test)]
mod tests;
