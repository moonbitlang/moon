# Split Read Job Guest Destination From Host Buffer

Moonrun read jobs will keep guest destination metadata on the stable job state while workers read into host-owned buffers. Completion records the produced byte count, and moonrun copies the host buffer into current guest memory only when the guest resumes. This preserves the native read-job contract without retaining guest-memory pointers in worker threads.

Windows overlapped `IOResult` values follow the same boundary. Read result creation allocates host buffers from lengths only; it must not receive or retain a MoonBit array destination. Write result creation copies the MoonBit source bytes into host-owned storage before returning the opaque result handle. Socket-with-address results also copy the MoonBit address into host-owned storage at creation time, because `WSARecvFrom` and `WSASendTo` need stable `sockaddr` and length storage while the overlapped operation is pending.

Read/write submission and status polling do not receive MoonBit data arrays. They only start or observe the OS operation. Once a read operation has completed, MoonBit calls an explicit copy-out import that moves the completed host buffer contents into the current guest array. `recvfrom` uses a separate copy-out import that also receives the MoonBit address buffer. Writes never copy data back.

Native Windows keeps the same lifetime by retaining the MoonBit buffer object on the `IOResult` until `free_io_result`; socket-with-address results retain the address object too. The wasm host cannot hold guest pointers across imports, so every payload or address that may be used by a pending operation must be copied into the host-owned `IOResult` at construction time.
