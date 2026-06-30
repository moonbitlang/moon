# Split Read Job Guest Destination From Host Buffer

Moonrun read jobs will keep guest destination metadata on the stable job state while workers read into host-owned buffers. Completion records the produced byte count, and moonrun copies the host buffer into current guest memory only when the guest resumes. This preserves the native read-job contract without retaining guest-memory pointers in worker threads.

Windows overlapped `IOResult` values follow the same boundary. Result creation allocates host buffers from lengths only; it must not receive or retain a MoonBit array destination. Socket-with-address results also copy the MoonBit address into host-owned storage at creation time, because `WSARecvFrom` and `WSASendTo` need stable `sockaddr` and length storage while the overlapped operation is pending.

Read submission and status polling do not receive MoonBit destinations. They only start or observe the OS operation. Once a read operation has completed, MoonBit calls an explicit copy-out import that moves the completed host buffer contents, and any completed socket peer address, into the current guest arrays. Write submission copies the current guest source into the host buffer before issuing the overlapped operation and never copies it back.
