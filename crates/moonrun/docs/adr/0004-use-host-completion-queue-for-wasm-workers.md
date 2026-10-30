# Use Host Completion Queue For Wasm Workers

Moonrun will expose a native-shaped completion-drain import for wasm async workers rather than forcing an OS notify fd into the V8 adapter. The MoonBit side keeps the `fetch_completion` shape, while moonrun can implement the queue with host synchronization primitives and fill guest buffers only during the drain call.

Worker threads must not write directly into wasm guest memory. They do not run inside a V8 import callback, and the current memory view must be reacquired after potential memory growth. Worker jobs therefore copy borrowed inputs into host-owned values at job creation, compute host-owned result payloads, publish a completion record, and let the guest thread copy output bytes while draining completions.

This delayed copy-out is part of the boundary design for output buffers such as read, readdir, and file-time results. The MoonBit wasm wrapper treats jobs as opaque handles as the native backend does; `fetch_completion` drains ready job ids, and payload-specific result imports copy host-owned payloads only after the guest coroutine resumes. Fixed-size portable records should still avoid unnecessary intermediate structure: wasm `FileTime` is a 48-byte little-endian record, so whichever guest-thread import copies it should encode that record directly into guest memory instead of exposing per-field host accessors.
