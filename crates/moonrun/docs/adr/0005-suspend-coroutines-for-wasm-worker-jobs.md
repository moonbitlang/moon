# Suspend Coroutines For Wasm Worker Jobs

Moonrun will make wasm worker jobs suspend the waiting coroutine once the real executor is implemented. The current synchronous `run_job` path is only a temporary runnable slice; leaving it as observable behavior would block the guest event loop and diverge from `moonbitlang/async` native semantics.
