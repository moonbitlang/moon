# Copy Worker Results Through Host Completions

Moonrun will not let async workers retain guest-memory pointers or write directly into guest memory after an import returns. Worker jobs produce host-owned completions, and guest-visible result data is copied into current guest memory only when the guest resumes and calls back into moonrun. This adds a copy, but keeps wasm memory growth/replacement and worker-thread execution separated.
