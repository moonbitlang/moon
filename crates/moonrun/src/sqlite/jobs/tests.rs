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

use super::*;
use crate::runtime::Runtime;
use crate::sqlite::tests::{open_memory, runtime, utf16le};

fn take_handle(runtime: &Runtime, job: u64) -> SqliteHostResult<u64> {
    runtime
        .async_host()
        .with_job_mut(job, |job| {
            runtime.sqlite().take_job_handle(job.sqlite_mut().unwrap())
        })
        .unwrap()
}

#[test]
fn shared_jobs_transfer_results_once_and_finalizers_consume_statements() {
    let runtime = runtime();
    let host = runtime.sqlite();
    let pool = runtime.async_host();
    let job = pool
        .insert_job(host.make_open_job(
            c":memory:".to_owned(),
            ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE,
        ))
        .unwrap();
    pool.run_job(job).unwrap();
    assert_eq!(pool.job_get_ret(job), Ok(i64::from(ffi::SQLITE_OK)));
    assert_eq!(pool.job_get_err(job), Ok(0));
    let database = take_handle(&runtime, job).unwrap();
    assert_eq!(
        take_handle(&runtime, job),
        Err(SqliteHostError::InvalidInput)
    );
    pool.free_job(job).unwrap();

    let prepare = pool
        .insert_job(
            host.make_prepare_job(database, utf16le("SELECT 42; SELECT 43"))
                .unwrap(),
        )
        .unwrap();
    pool.run_job(prepare).unwrap();
    let mut result = [0; 24];
    pool.with_job(prepare, |job| {
        job.sqlite().unwrap().copy_result(&mut result)
    })
    .unwrap()
    .unwrap();
    assert_eq!(i32::from_le_bytes(result[8..12].try_into().unwrap()), 10);
    let statement = take_handle(&runtime, prepare).unwrap();
    assert_eq!(
        take_handle(&runtime, prepare),
        Err(SqliteHostError::InvalidInput)
    );
    pool.free_job(prepare).unwrap();
    let finalizer = pool
        .insert_job(host.make_finalize_job(database, statement).unwrap())
        .unwrap();
    assert_eq!(host.step(statement), Err(SqliteHostError::InvalidHandle));
    pool.run_job(finalizer).unwrap();
    assert_eq!(pool.job_get_ret(finalizer), Ok(i64::from(ffi::SQLITE_OK)));
    pool.free_job(finalizer).unwrap();
    host.close(database).unwrap();
    assert!(host.leak_summary().is_none());
}

#[test]
fn detached_submitted_jobs_keep_their_sqlite_inputs_alive() {
    let runtime = runtime();
    let host = runtime.sqlite();
    let pool = runtime.async_host();
    let database = open_memory(host);
    let statement = host
        .prepare16_v2(database, &utf16le("SELECT 1"))
        .unwrap()
        .statement
        .unwrap();
    let poll = pool.poll_create().unwrap();
    pool.init_thread_pool(poll).unwrap();
    let guard = host.database(database).unwrap().lock();
    let job = pool
        .insert_job(host.make_step_job(database, statement).unwrap())
        .unwrap();
    let worker = pool.spawn_worker(42, job).unwrap();
    pool.free_job(job).unwrap();
    assert_eq!(pool.job_get_ret(job), Err(AsyncHostError::Badf));
    assert_eq!(host.close(database), Err(SqliteHostError::InvalidInput));
    assert_eq!(host.finalize(statement), Err(SqliteHostError::InvalidInput));
    assert_eq!(host.reset(statement), Err(SqliteHostError::InvalidInput));
    assert_eq!(
        host.column_count(statement),
        Err(SqliteHostError::InvalidInput)
    );
    assert_eq!(
        host.bind_null(statement, 1),
        Err(SqliteHostError::InvalidInput)
    );
    assert_eq!(
        host.clear_bindings(statement),
        Err(SqliteHostError::InvalidInput)
    );
    drop(guard);
    pool.free_worker(worker).unwrap();
    host.finalize(statement).unwrap();
    host.close(database).unwrap();
    pool.destroy_thread_pool();
    pool.poll_destroy(poll).unwrap();
}

#[test]
fn idle_statement_metadata_does_not_wait_for_the_connection_mutex() {
    let host = crate::sqlite::tests::host();
    let database = open_memory(&host);
    let statement = host
        .prepare16_v2(database, &utf16le("SELECT 1"))
        .unwrap()
        .statement
        .unwrap();
    let pointer = DatabasePointer(host.database(database).unwrap());
    let (locked_tx, locked_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let database = pointer;
        let _guard = database.0.lock();
        locked_tx.send(()).unwrap();
        // Bound a regression's wait: a hidden lock in statement lookup would
        // block column_count until this timeout releases the native mutex.
        release_rx.recv_timeout(std::time::Duration::from_secs(5))
    });
    locked_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let count = host.column_count(statement);
    let _ = release_tx.send(());
    assert_eq!(worker.join().unwrap(), Ok(()));
    assert_eq!(count, Ok(1));
    host.finalize(statement).unwrap();
    host.close(database).unwrap();
}

#[test]
fn connection_error_reads_observe_intervening_jobs() {
    let runtime = runtime();
    let host = runtime.sqlite();
    let pool = runtime.async_host();
    let database = open_memory(host);
    assert_eq!(
        host.prepare16_v2(database, &utf16le("SELECT missing_column"))
            .unwrap()
            .code,
        ffi::SQLITE_ERROR
    );
    assert_eq!(host.errcode(database), Ok(ffi::SQLITE_ERROR));
    let prepare = pool
        .insert_job(
            host.make_prepare_job(database, utf16le("SELECT 42"))
                .unwrap(),
        )
        .unwrap();
    pool.run_job(prepare).unwrap();
    // A safe read is not a snapshot of the preceding guest operation.
    assert_eq!(host.errcode(database), Ok(ffi::SQLITE_OK));
    assert_eq!(host.extended_errcode(database), Ok(ffi::SQLITE_OK));
    let statement = take_handle(&runtime, prepare).unwrap();
    pool.free_job(prepare).unwrap();
    host.finalize(statement).unwrap();
    host.close(database).unwrap();
}

#[test]
fn detached_preparing_jobs_destroy_unclaimed_results_before_releasing_the_database() {
    let runtime = runtime();
    let host = runtime.sqlite();
    let pool = runtime.async_host();
    let database = open_memory(host);
    let poll = pool.poll_create().unwrap();
    pool.init_thread_pool(poll).unwrap();
    let guard = host.database(database).unwrap().lock();
    let job = pool
        .insert_job(
            host.make_prepare_job(database, utf16le("SELECT 42"))
                .unwrap(),
        )
        .unwrap();
    let worker = pool.spawn_worker(1, job).unwrap();
    pool.free_job(job).unwrap();
    assert_eq!(pool.job_get_ret(job), Err(AsyncHostError::Badf));
    assert_eq!(host.close(database), Err(SqliteHostError::InvalidInput));
    let replacement = pool
        .insert_job(crate::async_sys::internal::event_loop::thread_pool::make_sleep_job(0))
        .unwrap();
    drop(guard);
    assert_eq!(pool.poll_wait(poll, 10_000), Ok(1));
    pool.free_worker(worker).unwrap();
    assert_eq!(pool.job_get_ret(job), Err(AsyncHostError::Badf));
    pool.run_job(replacement).unwrap();
    pool.free_job(replacement).unwrap();
    // SQLITE_BUSY here would reveal an unclaimed native Statement leak.
    assert_eq!(host.close(database), Ok(ffi::SQLITE_OK));
    pool.destroy_thread_pool();
    pool.poll_destroy(poll).unwrap();
}

#[test]
fn freeing_jobs_destroys_unclaimed_statements_and_unrun_finalizers() {
    let runtime = runtime();
    let host = runtime.sqlite();
    let pool = runtime.async_host();
    let database = open_memory(host);
    let prepare = pool
        .insert_job(
            host.make_prepare_job(database, utf16le("SELECT 42"))
                .unwrap(),
        )
        .unwrap();
    pool.run_job(prepare).unwrap();
    pool.free_job(prepare).unwrap();
    assert_eq!(host.close(database), Ok(ffi::SQLITE_OK));

    let database = open_memory(host);
    let statement = host
        .prepare16_v2(database, &utf16le("SELECT 42"))
        .unwrap()
        .statement
        .unwrap();
    let finalizer = pool
        .insert_job(host.make_finalize_job(database, statement).unwrap())
        .unwrap();
    assert_eq!(host.close(database), Err(SqliteHostError::InvalidInput));
    pool.free_job(finalizer).unwrap();
    assert_eq!(host.step(statement), Err(SqliteHostError::InvalidHandle));
    assert_eq!(host.close(database), Ok(ffi::SQLITE_OK));
    assert!(host.leak_summary().is_none());
}

#[test]
fn idle_async_workers_can_run_sqlite_and_existing_jobs() {
    let runtime = runtime();
    let pool = runtime.async_host();
    let host = runtime.sqlite();
    let poll = pool.poll_create().unwrap();
    pool.init_thread_pool(poll).unwrap();
    let sleep = pool
        .insert_job(crate::async_sys::internal::event_loop::thread_pool::make_sleep_job(0))
        .unwrap();
    let worker = pool.spawn_worker(1, sleep).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while pool.job_get_ret(sleep) != Ok(0) {
        assert!(std::time::Instant::now() < deadline);
        pool.poll_wait(poll, 10).unwrap();
    }
    pool.free_job(sleep).unwrap();
    pool.worker_enter_idle(worker).unwrap();

    let database = open_memory(host);
    let job = pool
        .insert_job(
            host.make_prepare_job(database, utf16le("SELECT 42"))
                .unwrap(),
        )
        .unwrap();
    pool.wake_worker(worker, 2, job).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !pool
        .with_job(job, |job| job.sqlite().unwrap().result.is_some())
        .unwrap_or(false)
    {
        assert!(
            std::time::Instant::now() < deadline,
            "reused worker did not complete SQLite job"
        );
        pool.poll_wait(poll, 10).unwrap();
        std::thread::yield_now();
    }
    pool.free_worker(worker).unwrap();
    let statement = take_handle(&runtime, job).unwrap();
    pool.free_job(job).unwrap();
    host.finalize(statement).unwrap();
    host.close(database).unwrap();
    pool.destroy_thread_pool();
    pool.poll_destroy(poll).unwrap();
}

#[test]
fn captured_diagnostics_survive_other_operations_on_the_connection() {
    let runtime = runtime();
    let pool = runtime.async_host();
    let host = runtime.sqlite();
    let database = open_memory(host);
    let failed = pool
        .insert_job(
            host.make_prepare_job(database, utf16le("SELECT missing_column"))
                .unwrap(),
        )
        .unwrap();
    pool.run_job(failed).unwrap();
    assert_eq!(pool.job_get_ret(failed), Ok(i64::from(ffi::SQLITE_ERROR)));
    assert_eq!(pool.job_get_err(failed), Ok(0));
    let statement = host
        .prepare16_v2(database, &utf16le("SELECT 42"))
        .unwrap()
        .statement
        .unwrap();
    host.step(statement).unwrap();
    let message = pool
        .with_job(failed, |job| {
            let job = job.sqlite().unwrap();
            let mut output = vec![0; job.message16_length().unwrap() as usize];
            job.copy_message16(&mut output).unwrap();
            String::from_utf16(&output).unwrap()
        })
        .unwrap();
    assert!(message.contains("missing_column"));
    pool.free_job(failed).unwrap();
    host.finalize(statement).unwrap();
    host.close(database).unwrap();
}

#[test]
fn invalid_finalizer_input_preserves_the_statement() {
    let runtime = runtime();
    let host = runtime.sqlite();
    let first = open_memory(host);
    let other = open_memory(host);
    let statement = host
        .prepare16_v2(other, &utf16le("SELECT 42"))
        .unwrap()
        .statement
        .unwrap();
    assert!(matches!(
        host.make_finalize_job(first, statement),
        Err(SqliteHostError::InvalidInput)
    ));
    assert_eq!(host.step(statement), Ok(ffi::SQLITE_ROW));
    host.finalize(statement).unwrap();
    host.close(first).unwrap();
    host.close(other).unwrap();
}

#[test]
fn runtime_releases_guest_mutexes_before_joining_shared_sqlite_workers() {
    let runtime = runtime();
    let host = runtime.sqlite();
    let pool = runtime.async_host();
    let database = open_memory(host);
    let poll = pool.poll_create().unwrap();
    pool.init_thread_pool(poll).unwrap();
    let job = pool
        .insert_job(
            host.make_prepare_job(database, utf16le("SELECT 1"))
                .unwrap(),
        )
        .unwrap();
    let mutex = host.db_mutex(database).unwrap();
    host.mutex_enter(mutex).unwrap();
    pool.spawn_worker(1, job).unwrap();
    // Abandon the job, result, Database, poll, and guest mutex entry. Teardown
    // must join the shared worker before destroying any SQLite pointers.
    drop(runtime);
}

#[test]
fn discard_keeps_an_unclaimed_statement_on_the_host_and_retains_its_database() {
    let runtime = runtime();
    let host = runtime.sqlite();
    let pool = runtime.async_host();
    let database = open_memory(host);
    let prepare = pool
        .insert_job(
            host.make_prepare_job(database, utf16le("SELECT 1; SELECT 2"))
                .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        pool.with_job_mut(prepare, |job| host
            .make_discard_job(job.sqlite_mut().unwrap()))
            .unwrap(),
        Err(SqliteHostError::InvalidInput)
    ));
    pool.run_job(prepare).unwrap();
    let discard = pool
        .with_job_mut(prepare, |job| {
            host.make_discard_job(job.sqlite_mut().unwrap())
        })
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(host.statements.borrow().is_empty());
    assert_eq!(
        take_handle(&runtime, prepare),
        Err(SqliteHostError::InvalidInput)
    );
    assert!(
        pool.with_job_mut(prepare, |job| host
            .make_discard_job(job.sqlite_mut().unwrap()))
            .unwrap()
            .unwrap()
            .is_none()
    );
    // Result snapshots survive disposal; only the owned pointer moves.
    let mut result = [0; 24];
    pool.with_job(prepare, |job| {
        job.sqlite().unwrap().copy_result(&mut result)
    })
    .unwrap()
    .unwrap();
    assert_eq!(i32::from_le_bytes(result[8..12].try_into().unwrap()), 9);
    pool.free_job(prepare).unwrap();
    let discard = pool.insert_job(discard).unwrap();
    assert_eq!(host.close(database), Err(SqliteHostError::InvalidInput));
    pool.run_job(discard).unwrap();
    pool.free_job(discard).unwrap();
    host.close(database).unwrap();
    assert!(host.leak_summary().is_none());
}

#[test]
fn unclaimed_open_results_are_discarded_without_publishing_database_handles() {
    let runtime = runtime();
    let host = runtime.sqlite();
    let pool = runtime.async_host();
    let open = pool
        .insert_job(host.make_open_job(
            c":memory:".to_owned(),
            ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE,
        ))
        .unwrap();
    pool.run_job(open).unwrap();
    let discard = pool
        .with_job_mut(open, |job| host.make_discard_job(job.sqlite_mut().unwrap()))
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(host.databases.borrow().is_empty());
    assert_eq!(
        take_handle(&runtime, open),
        Err(SqliteHostError::InvalidInput)
    );
    pool.free_job(open).unwrap();
    let discard = pool.insert_job(discard).unwrap();
    pool.run_job(discard).unwrap();
    assert_eq!(pool.job_get_ret(discard), Ok(i64::from(ffi::SQLITE_OK)));
    pool.free_job(discard).unwrap();
    assert!(host.leak_summary().is_none());
}
