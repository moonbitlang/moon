# Windows timezone imports

The following operations in `moonbitlang/core` mirror the Win32 reads used by
`x/time`. Each returns an i32 native error code, zero on success. Addresses are
unsigned byte offsets into linear memory, need no alignment, and are checked
for the full output size before the operation. Errors leave outputs unchanged.
Unix hosts register the imports so a portable binary can select its platform
at runtime; calls with available memory return `ENOTSUP`.

| Import (prefix `time/windows/`) | Wasm parameters | Successful output |
| --- | --- | --- |
| `get_dynamic_time_zone_information` | `out_handle: i32` | One opaque u64 handle (8 bytes). |
| `get_time_zone_metadata` | `handle: i64, out: i32, capacity: i32` | u32 dynamic-DST-disabled flag, u32 key-name byte length, then exactly that many UTF-16LE bytes. |
| `get_current_time_zone_rule` | `handle: i64, out: i32` | The captured current rule (44 bytes). |
| `get_dynamic_time_zone_information_effective_years` | `handle: i64, out: i32` | Two u32 values, inclusive first and last years. Win32 returns `ERROR_FILE_NOT_FOUND` if it provides no range. |
| `get_time_zone_information_for_year` | `handle: i64, year: i32, out: i32` | One 44-byte rule. The unsigned year must fit a Win32 USHORT. |
| `free_time_zone` | `handle: i64` | Releases the capture. Invalid, wrong-kind, and retired handles return `ERROR_INVALID_HANDLE` on Windows. |

Capture acquires the handle. All data queries take that handle and write only
their own result. The metadata call checks the unsigned capacity against guest memory.
It returns `ERROR_INSUFFICIENT_BUFFER` if the result does not fit. A 264-byte
buffer covers the Win32 maximum of 128 UTF-16 code units plus the header.
The name has no terminator; an empty key has length zero. Bytes beyond the name
are left untouched. Future queries can be added independently of these layouts.

All fields are little-endian. A rule has three signed i32 minute biases
(base, standard, daylight), followed by standard and daylight transition dates.
Each date has eight u16 fields in this order: year, month, weekday, day, hour,
minute, second, millisecond. A guest decides how to interpret these fields.

Years are full calendar years, not two-digit years or offsets from an epoch.
The year query checks that its unsigned input fits `USHORT` before calling
Windows; it never truncates the input. Representability does not guarantee OS
support: the native call can still fail. In transition dates, year zero denotes
a recurring rule and month zero denotes an absent transition, as in Win32.
Biases retain their signed minute units. Guests should widen before adding
biases or converting to seconds and use signed 64-bit seconds for calculated
timestamps. No epoch timestamp is stored in the rule record.

The Runtime owns the original Win32 capture. Editing the output bytes cannot
change the key used for subsequent queries. Queries use that capture with the
system's installed rule data; the OS does not provide an atomic registry
transaction across calls. The guest should release the handle after loading
the rules, including on errors. Runtime teardown releases abandoned handles.
