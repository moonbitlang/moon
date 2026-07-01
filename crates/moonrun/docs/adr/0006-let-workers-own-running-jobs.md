# Let Workers Own Running Jobs

Moonrun will follow `moonbitlang/async` by treating a worker as owning execution of its current running job while the blocking operation executes. The Async Host still owns the stable guest-visible job handle and reservation state, but moonrun should not require central job-table locks around blocking syscalls; completion publishes the finished job state back for the guest event loop to observe.
