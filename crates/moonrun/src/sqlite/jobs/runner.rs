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

//! One-shot native work. No guest Handles or Guest Memory cross this boundary.
use std::ffi::CString;
use std::ptr::NonNull;
use std::sync::Arc;

use crate::filesystem::HostFs;
use crate::sqlite::connection::{Database, open_database, utf16_string_length};
use crate::sqlite::statement::prepare_statement;
use libsqlite3_sys as ffi;

/// The owning Job keeps its input lease alive, including after the guest frees
/// an in-flight Job. Runtime destroys Async Host workers and Jobs before SQLite
/// tables. FULLMUTEX connections serialize all native access.
#[derive(Clone, Copy, Debug)]
pub(super) struct DatabasePointer(pub(super) Database);
unsafe impl Send for DatabasePointer {}
#[derive(Clone, Copy, Debug)]
pub(super) struct StatementPointer(pub(super) NonNull<ffi::sqlite3_stmt>);
unsafe impl Send for StatementPointer {}

#[derive(Debug)]
pub(super) struct OwnedDatabase(pub(super) Option<DatabasePointer>);
impl Drop for OwnedDatabase {
    fn drop(&mut self) {
        if let Some(database) = self.0.take() {
            unsafe { ffi::sqlite3_close(database.0.pointer().as_ptr()) };
        }
    }
}
#[derive(Debug)]
pub(super) struct OwnedStatement(pub(super) Option<StatementPointer>);
impl Drop for OwnedStatement {
    fn drop(&mut self) {
        if let Some(statement) = self.0.take() {
            unsafe { ffi::sqlite3_finalize(statement.0.as_ptr()) };
        }
    }
}

pub(super) enum Input {
    Close(OwnedDatabase),
    Open {
        filesystem: Arc<HostFs>,
        filename: CString,
        flags: i32,
    },
    Prepare {
        database: DatabasePointer,
        sql: Vec<u16>,
    },
    Step {
        database: DatabasePointer,
        statement: StatementPointer,
    },
    Finalize {
        database: DatabasePointer,
        statement: OwnedStatement,
    },
}

#[derive(Debug)]
pub(super) enum Output {
    Open(OwnedDatabase),
    Prepare {
        statement: OwnedStatement,
        tail: i32,
    },
    Step(Option<i64>),
    Finalize,
}

#[derive(Debug)]
pub(super) struct Outcome {
    pub(super) code: i32,
    pub(super) extended: i32,
    pub(super) message: Vec<u16>,
    pub(super) output: Output,
}

unsafe extern "C" {
    fn sqlite3_errmsg16(database: *mut ffi::sqlite3) -> *const std::ffi::c_void;
}

impl Outcome {
    fn new(code: i32, output: Output) -> Self {
        Self {
            code,
            extended: code,
            message: Vec::new(),
            output,
        }
    }

    /// Caller holds the connection mutex through the operation and this copy.
    fn capture_error(&mut self, database: Database) {
        self.extended = unsafe { ffi::sqlite3_extended_errcode(database.pointer().as_ptr()) };
        let message = unsafe { sqlite3_errmsg16(database.pointer().as_ptr()) };
        if let Ok(Some(length)) = unsafe { utf16_string_length(message) } {
            self.message =
                unsafe { std::slice::from_raw_parts(message.cast(), length as usize) }.to_vec();
        }
    }
}

impl Input {
    pub(super) fn run(self) -> Outcome {
        match self {
            Self::Close(database) => {
                // This Database was never exposed to guest code and has no
                // Statements. Close on the worker; no mutex guard may outlive it.
                drop(database);
                Outcome::new(ffi::SQLITE_OK, Output::Finalize)
            }
            Self::Open {
                filesystem,
                filename,
                flags,
            } => {
                let (code, database) =
                    open_database(&filesystem, &filename, flags, crate::runtime::null_handle());
                let mut outcome = Outcome::new(code, Output::Open(OwnedDatabase(None)));
                if let Some(database) = database {
                    if code == ffi::SQLITE_OK {
                        outcome.output =
                            Output::Open(OwnedDatabase(Some(DatabasePointer(database))));
                    } else {
                        {
                            let _guard = database.lock();
                            outcome.capture_error(database);
                        }
                        unsafe { ffi::sqlite3_close(database.pointer().as_ptr()) };
                    }
                }
                outcome
            }
            Self::Prepare {
                database: DatabasePointer(database),
                sql,
            } => {
                let _guard = database.lock();
                let (code, statement, tail) = match prepare_statement(database, &sql) {
                    Ok(result) => result,
                    Err(_) => {
                        return Outcome::new(
                            ffi::SQLITE_TOOBIG,
                            Output::Prepare {
                                statement: OwnedStatement(None),
                                tail: -1,
                            },
                        );
                    }
                };
                let tail = if statement.is_some() { tail as i32 } else { -1 };
                let mut outcome = Outcome::new(
                    code,
                    Output::Prepare {
                        statement: OwnedStatement(statement.map(StatementPointer)),
                        tail,
                    },
                );
                if code != ffi::SQLITE_OK {
                    outcome.capture_error(database);
                    outcome.output = Output::Prepare {
                        statement: OwnedStatement(None),
                        tail: -1,
                    };
                }
                outcome
            }
            Self::Step {
                database: DatabasePointer(database),
                statement: StatementPointer(statement),
            } => {
                let _guard = database.lock();
                let read_only = unsafe { ffi::sqlite3_stmt_readonly(statement.as_ptr()) } != 0;
                let before = unsafe { ffi::sqlite3_total_changes64(database.pointer().as_ptr()) };
                let code = unsafe { ffi::sqlite3_step(statement.as_ptr()) };
                let changes = if code == ffi::SQLITE_DONE && !read_only {
                    let after =
                        unsafe { ffi::sqlite3_total_changes64(database.pointer().as_ptr()) };
                    Some(if before == after {
                        0
                    } else {
                        unsafe { ffi::sqlite3_changes64(database.pointer().as_ptr()) }
                    })
                } else {
                    None
                };
                let mut outcome = Outcome::new(code, Output::Step(changes));
                if code != ffi::SQLITE_ROW && code != ffi::SQLITE_DONE {
                    outcome.capture_error(database);
                }
                outcome
            }
            Self::Finalize {
                database: DatabasePointer(database),
                mut statement,
            } => {
                let _guard = database.lock();
                let pointer = statement
                    .0
                    .take()
                    .expect("submitted finalizer owns a statement");
                let code = unsafe { ffi::sqlite3_finalize(pointer.0.as_ptr()) };
                let mut outcome = Outcome::new(code, Output::Finalize);
                if code != ffi::SQLITE_OK {
                    outcome.capture_error(database);
                }
                outcome
            }
        }
    }
}
