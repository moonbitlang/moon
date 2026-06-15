# Async Wasm Runtime Audit TODO

## Narrative

And PR #1772 adds the `moonrun` host runtime needed for `moonbitlang/async` on the wasm backend, with a declared MVP surface around filesystem, fd, c_buffer, os_error, env, time, thread_pool, and limited process support.

But upstream async wasm tests have been failing, and serializing timing-sensitive tests can hide scheduler or ABI bugs rather than proving the Rust host runtime faithfully preserves the native C-stub semantics.

Therefore prioritize ABI, ownership, and completion correctness before relying on test serialization as evidence of stability.

## Priority List

1. [ ] Fix ABI and guest-memory lifetime safety.
   And wasm jobs pass borrowed guest pointers that must remain valid until host completion.
   But `Job::file_time_by_path` does not retain the output `FileTime`, and several copy-out paths are not transactional.
   Therefore retain every guest owner needed by pending jobs and make `run_job`, `fetch_completion`, and `pipe` validate or roll back on copy-out failure.

2. [ ] Prevent hangs and lost completions.
   And the wasm event loop depends on every running worker job eventually publishing a completion.
   But stale or freed job handles can currently make a worker return without queueing a completion.
   Therefore make worker failure paths publish a completion or surface a deterministic error so the event loop cannot wait forever.

3. [ ] Audit supported-surface parity only.
   And the PR intentionally supports an MVP subset of async host imports.
   But poll, direct IO, sockets, TLS, named pipes, and some spawn-job APIs are registered as unsupported.
   Therefore verify parity for the supported surface and document unsupported imports as explicit scope boundaries.

4. [ ] Check process behavior separately.
   And wasm process support is partly custom glue rather than a direct C-stub port.
   But Unix signaled child status, Windows argv quoting, Windows handle inheritance, and silently ignored spawn options can diverge from native behavior.
   Therefore either match native semantics or fail loudly for unsupported process options.

5. [ ] Stabilize tests after correctness fixes.
   And upstream async tests include timing-sensitive cases.
   But `--no-parallelize` or `--max-concurrent-tests` only reduces scheduling pressure.
   Therefore apply serialization as a stability measure after ABI and completion bugs are addressed.

6. [ ] Add focused regression coverage.
   And upstream package success gives useful end-to-end confidence.
   But it does not stress malformed guest ranges, too-small logical buffers, failed copy-outs, lost completions, or ownership mistakes.
   Therefore add targeted tests for invalid guest buffers, file-time copy-out, completion fetch failure, pipe output failure, `file_time_by_path` ownership, and process edge cases.
