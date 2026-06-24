# Use Native-Shaped Worker Threads First

Moonrun will implement the first wasm async executor with one host thread per active native-shaped worker handle, subject to the same worker-count policy as `moonbitlang/async`. This favors behavioral parity and easier cancellation reasoning over starting with a more abstract shared Rust executor pool.
