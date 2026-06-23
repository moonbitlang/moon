# Split Read Job Guest Destination From Host Buffer

Moonrun read jobs will keep guest destination metadata on the stable job state while workers read into host-owned buffers. Completion records the produced byte count, and moonrun copies the host buffer into current guest memory only when the guest resumes. This preserves the native read-job contract without retaining guest-memory pointers in worker threads.
