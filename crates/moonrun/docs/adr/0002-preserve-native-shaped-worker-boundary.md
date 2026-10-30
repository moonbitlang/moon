# Preserve Native-Shaped Worker Boundary

Moonrun will preserve the `moonbitlang/async` native-shaped `Worker` and `Job` boundary for wasm instead of exposing a separate wasm-specific scheduler API to MoonBit. In wasm, worker handles may represent scheduler resources rather than raw OS threads, but the MoonBit-facing structure should stay close to the native async implementation to reduce maintenance drift.

When checking parity with native behavior, evaluate the native C stubs together
with the MoonBit code that wraps them. The compatibility target is the behavior
reachable through the `moonbitlang/async` MoonBit API, not every possible misuse
of a raw C symbol. If the MoonBit layer constrains ownership or lifecycle, such
as creating a private `Job`, submitting it once, reading the result, and freeing
it, the wasm host should model that contract explicitly and document any
deliberately stricter checks against raw C misuse.

For worker jobs, the wasm host treats a job handle as a one-shot result handle.
A newly-created job is ready to submit once. Submitting it to a worker moves the
payload out of the guest-visible table and leaves a reservation slot while the
host worker runs. Completion restores the payload as result-readable, so
`job_get_ret`, `job_get_err`, and payload-specific result accessors remain valid
until `free_job`, but `run_job`, `spawn_worker`, and `wake_worker` reject the
completed handle. Queued jobs remain cancellable or displaceable until a worker
takes them; freeing a queued or running job discards the result payload when the
worker later reports completion.
