# Core environment snapshot ABI

These synchronous imports in `moonbitlang/core` use the Runtime's cwd and Env.
Byte addresses and counts are wasm32 `i32` values interpreted as unsigned.
Reads return an opaque `i64` CBuffer Handle; mutations return an `i32` OS error
code, with zero meaning success.

| Import | Parameters | Result |
| --- | --- | --- |
| `env/current_dir` | none | snapshot Handle |
| `env/get_env_var` | `key_ptr, key_len` | snapshot Handle |
| `env/set_env_var` | `key_ptr, key_len, value_ptr, value_len` | OS error code |
| `env/unset_env_var` | `key_ptr, key_len` | OS error code |

## Snapshot ownership

A successful read copies its result into a new host-owned CBuffer. Subsequent
changes to cwd or Env, including removal of the variable, do not change that
buffer. The guest uses existing `moonbitlang/async` imports to consume it:

1. `c_buffer/is_null(handle)` distinguishes failure from a live result. Do not
   compare the Handle with zero: null is the runtime's encoded null key.
2. `c_buffer/length(handle)` returns its native-string byte count as `i32`/`u32`.
3. `c_buffer/blit_from_c(handle, 0, buffer_ptr, 0, length)` copies those bytes
   into freshly acquired Guest Memory. Copying does not consume the Handle.
4. `c_buffer/free(handle)` releases the result. Guest wrappers must release it
   on success and error propagation; Run teardown releases remaining buffers.

Length and copy operate on the same snapshot, without querying the environment
or cwd again. No size-query retry is needed. These are ordinary CBuffers: a
guest can explicitly mutate them with the existing buffer operations, but that
does not mutate the Runtime's Env or cwd. Buffer bounds and stale or wrong-kind
Handles retain the existing CBuffer validation and error behavior.

A missing variable returns null; an existing empty value returns a live Handle
whose length is zero. Failed reads record the host OS error for
`os_error/get_errno`, matching `env/current_exe`. Read the error after detecting
null, before another fallible host operation. Successful reads do not clear a
previous error. Set/unset return their OS error directly and do not set async's
last-error state.

## Strings and errors

Inputs and snapshots use the host's native string representation without a
NUL terminator or BOM:

| Platform | Representation |
| --- | --- |
| Unix | Original bytes; no UTF-8 validation or transcoding |
| Windows | Little-endian 16-bit code units; unpaired surrogates are preserved |

All lengths are byte counts on both platforms. Windows input lengths must be
even; addresses need not be aligned. The ABI neither validates Unicode nor
replaces invalid sequences. In particular, leading U+FEFF is data. A guest may
keep the original bytes, decode strictly, or choose lossy text conversion.
Names, values and cwd all follow this rule, so raw environment values and names
can be read and written without losing information.

Set/unset reuse Env's native name/NUL validation and validate all input before
mutation. NUL means a zero byte on Unix or a zero 16-bit code unit on Windows,
not a zero byte within another Windows code unit. Lookup uses Env's existing
name rules: names it cannot look up behave like absent variables. All input
ranges must be readable; zero is a valid Guest Memory address. `current_dir`
requires no Guest Memory, including during Wasm instantiation.

| ABI condition | Unix error | Windows error |
| --- | --- | --- |
| Absent environment variable | `ENOENT` | `ERROR_ENVVAR_NOT_FOUND` |
| Incomplete 16-bit input code unit | Not applicable | `ERROR_INVALID_PARAMETER` |
| Invalid input memory range | `EFAULT` | `ERROR_INVALID_ADDRESS` |
| Invalid mutation name/value | `EINVAL` | `ERROR_INVALID_PARAMETER` |
| Result length exceeds `u32` | `EOVERFLOW` | `ERROR_ARITHMETIC_OVERFLOW` |
| Cwd read permission denied | `EACCES` | `ERROR_ACCESS_DENIED` |

Native failures retain their original OS code. The existing executable-path
imports use the same native encoding but keep their historical NUL terminator.
The new length-delimited snapshots omit that terminator. Text decoding belongs
to the consumer; requesting a snapshot does not perform it.

## Implementation and example

[`environment.mbt`](../../tests/test_cases/test_core_environment.in/main/environment.mbt)
contains a compiled guest wrapper: borrowed `Bytes` lower to linear-memory
addresses, `UInt64` carries the Handle, and `read_snapshot_bytes` allocates,
copies and frees it with `defer`. The separate `read_snapshot` text wrapper
chooses strict UTF-8 or UTF-16LE decoding in the guest.
[`main.mbt`](../../tests/test_cases/test_core_environment.in/main/main.mbt) checks
Unicode, raw non-Unicode values, empty/absent values, legacy interoperability,
cwd and snapshot stability after mutation and deletion. It also exercises a
guest decoding error after successfully obtaining a raw snapshot. `tests/core_environment.rs` compiles and executes
this fixture in debug and release with leak checking on the selected engine.
These are fixture wrappers; the toolchain's existing `@env` remains unchanged.

`core_api/registry.rs` centralizes import names, signatures, memory acquisition
and error delivery. `core_api/environment.rs` owns ABI validation and encoding,
calling `HostFs::current_directory` and the shared Runtime Env. Those owners
retain authorization, Ambient/Fixed cwd and Ambient/Owned environment semantics,
with OS mechanics below them. The Async Host's existing CBuffer table owns the
published snapshots; there is no separate handle registry or pending-result
cache. Ambient Env retains its embedder synchronization requirement.

The `__moonbit_*_unstable` and existing core executable-path imports retain
their original signatures, encodings and errors. Full environment enumeration
continues to use WASIp1 `environ_sizes_get` / `environ_get` with WASI errors.
