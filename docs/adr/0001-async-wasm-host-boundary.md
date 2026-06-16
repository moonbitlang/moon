# Async Wasm Host Boundary

We will support `moonbitlang/async` on the wasm backend through
`#cfg(target="wasm")` bindings to a `moonbit_v0` host module in `moonrun`,
without compiler changes or JS backend changes. The host follows the semantic
contract of async's native C stubs, while this first split only implements the
event-loop and timer foundation needed by the root async, async queue, and
semaphore tests.

## Decisions

- Use a semantic C-stub boundary: `moonbit_v0` symbols map from
  `moonbitlang_async_*` symbols by stripping that prefix and placing the leaf
  name under an async namespace.
- Keep the first split limited to event-loop and timer support. Filesystem,
  process, socket, TLS, raw-fd, c-buffer, and real worker-job behavior remain
  follow-up work.
- Use monotonic elapsed milliseconds with an unspecified process-local origin
  for wasm async time. The async timer contract uses differences between
  readings; the absolute epoch is not meaningful.
- Support Unix-family and Windows hosts first. Other host families are
  deliberately compile-time unsupported until the async C-stub parity target is
  defined for them.
- Keep link-required worker symbols registered when the wasm event-loop module
  references them, but make real worker/job operations fail loudly in this
  split instead of causing missing-import instantiation failures.

## PR1 Correspondence

| `moonbit_v0` import | Native async correspondence | PR1 status |
| --- | --- | --- |
| `runtime/exit` | wasm support glue | implemented |
| `runtime/get_platform` | `moonbitlang_async_get_platform` | implemented |
| `runtime/wait_for_event` | wasm support glue | implemented |
| `time/get_ms_since_epoch` | `moonbitlang_async_get_ms_since_epoch` | implemented with monotonic local origin |
| `os_error/get_errno` | `moonbitlang_async_get_errno` | implemented for host callback errno |
| `thread_pool/errno_is_cancelled` | `moonbitlang_async_errno_is_cancelled` | deterministic no-worker stub |
| `thread_pool/fetch_completion` | `moonbitlang_async_fetch_completion` | deterministic no-completion stub |
| `thread_pool/spawn_worker` | `moonbitlang_async_spawn_worker` | link-only unsupported stub |
| `thread_pool/free_worker` | `moonbitlang_async_free_worker` | link-only unsupported stub |
| `thread_pool/wake_worker` | `moonbitlang_async_wake_worker` | link-only unsupported stub |
| `thread_pool/worker_enter_idle` | `moonbitlang_async_worker_enter_idle` | link-only unsupported stub |
| `thread_pool/cancel_worker` | `moonbitlang_async_cancel_worker` | link-only unsupported stub |
| `thread_pool/free_job` | wasm support glue | link-only unsupported stub |
| `thread_pool/run_job` | wasm support glue | link-only unsupported stub |
| `thread_pool/job_get_ret` | `moonbitlang_async_job_get_ret` | link-only unsupported stub |
| `thread_pool/job_get_err` | `moonbitlang_async_job_get_err` | link-only unsupported stub |

## Deferred

Executor design, cancellation semantics, broad core FS, sockets, process, TLS,
file watching, arbitrary external fd/HANDLE adoption, Wasmtime/WasmEdge
adapters, and wasm-gc support remain follow-up work.
