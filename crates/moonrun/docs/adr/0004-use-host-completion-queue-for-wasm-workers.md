# Use Host Completion Queue For Wasm Workers

Moonrun will expose a native-shaped completion-drain import for wasm async workers rather than forcing an OS notify fd into the V8 adapter. The MoonBit side keeps the `fetch_completion` shape, while moonrun can implement the queue with host synchronization primitives and fill guest buffers only during the drain call.
