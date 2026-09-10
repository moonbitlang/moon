# Contributing Quick Start

## Developer workflows

- [SQLite jobs](sqlite-jobs.md): SQLite payloads in the shared async pool,
  result ownership, asynchronous discard, and upstream Wasm wrapper integration.
- [Porting `moonbitlang/async` changes](async-upstream-porting.md): audit the
  one-way upstream runtime-port request queue, preserve exact provenance,
  deliver one upstream port at a time, and mark the originating async pull
  request completed after the Moon change lands.

## Worker cancellation

The cancellation protocol follows upstream async #595. Workers acknowledge a
cancellation request before entering potentially blocking syscalls. A Unix
signal arriving after that check may precede the syscall, so the signal handler
writes the job ID into the same pipe used for normal completions. The MoonBit
event loop calls `cancel_worker_with_retry` both to start cancellation and to
check each job notification. The host retries cancellation while the Worker is
not yet `Waiting`, and returns a distinct status once the Job result is ready.

Native writes its shared Job result before setting `Waiting`. Moonrun preserves
that order using its existing result channel: send the host-owned Job, set
`Waiting`, then notify. This transfers Rust ownership within the same process;
it does not copy result buffers. Before accepting completion, the host restores
the result to its Job table. The guest wrapper copies output into Wasm memory
when it subsequently requests that output.

The historical `thread_pool/cancel_worker` import returns `RetryLater` for a
pending default Unix cancellation, so older guests retry after a timer. The new
`thread_pool/cancel_worker_with_retry` import opts into retry notifications and
combines native's cancellation call and completion check in one Wasm import.
It returns `0` for `RetryLater`, `1` for `NeedWait`, or `2` when the Worker is
`Waiting` and its result has been restored to the Job table. A Worker that has
acknowledged cancellation but not yet published its result still returns `1`.
The guest uses `2` to accept completion; the initial cancellation path still
waits for the completion notification to retire the Worker, as native does.
Older guests have no retry check and would treat retry notifications as
completed Jobs. Both imports take one `i64` Worker handle and return an `i32`
status; the historical import retains its original return values.

Worker freeing follows native's termination wakeup and join. Pipe capacity
beyond the guest scheduler's worker bound, and cancellation/draining after a
Run stops executing its guest event loop, remain correctness FIXMEs for a Run
teardown follow-up: pending retries or full pipes can prevent Workers from
exiting and make join hang. The backport does not add a separate notification
transport or a retry loop to `free_worker`.

## How to Build and Test

```bash
cargo build
cargo test
cargo test -p moonrun --no-default-features --features wasmtime
```

## Before PR

We encourage to add the following prefix to your commit message and PR title: feat, fix, internal, or minor.

It's recommended to run the following command before you submit a PR, which may help discover some potential ci failure ASAP

```bash
cargo fmt

cargo clippy --workspace --exclude moonrun --all-targets --all-features -- -D warnings
cargo clippy -p moonrun --all-targets -- -D warnings
cargo clippy -p moonrun --all-targets --no-default-features --features wasmtime -- -D warnings

cargo test
```

We use [typos](https://github.com/crate-ci/typos) to avoid potential typos, you can also download and run it locally before PR.
