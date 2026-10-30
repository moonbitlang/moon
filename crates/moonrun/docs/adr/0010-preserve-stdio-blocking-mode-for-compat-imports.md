# Preserve stdio blocking mode for compatibility imports

Moonrun keeps deprecated wasm imports such as `fd_util/set_nonblocking/unix` only so older `moonbitlang/async` wasm guests can instantiate. These compatibility imports must not change a descriptor's blocking mode, because an older guest may call them with stdin, stdout, or stderr handles whose blocking state belongs to the embedding process. Runtime-owned paths that create or register descriptors, such as pipe creation and poll registration, remain responsible for any nonblocking setup they require.
