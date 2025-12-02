fn moonbit_test_driver_internal_error_to_string(x : Error) -> String = "%error.to_string"

#cfg(target="native")
fn string_get(s: String, index: Int) -> Int = "%string_get"

#cfg(not(target="js"))
fn moonbit_unsafe_char_from_int(x : Int) -> Char = "%identity"

#cfg(target="native")
const MOONBIT_TEST_DRIVER_INTERNAL_IS_NATIVE = true

#cfg(not(target="native"))
const MOONBIT_TEST_DRIVER_INTERNAL_IS_NATIVE = false

// =========================================================
// ================== BEGIN WASM SPECIFIC ==================
// =========================================================

#cfg(any(target="wasm", target="wasm-gc"))
fn moonbit_test_driver_internal_get_file_name(file_name : MoonbitTestDriverInternalExternString) -> String {
  moonbit_test_driver_internal_string_from_extern(file_name)
}

#cfg(any(target="wasm", target="wasm-gc"))
#external
type MoonbitTestDriverInternalStringReadHandle

#cfg(any(target="wasm", target="wasm-gc"))
#external
type MoonbitTestDriverInternalExternString

#cfg(any(target="wasm", target="wasm-gc"))
fn moonbit_test_driver_internal_begin_read_string(s : MoonbitTestDriverInternalExternString) -> MoonbitTestDriverInternalStringReadHandle = "__moonbit_fs_unstable" "begin_read_string"

#cfg(any(target="wasm", target="wasm-gc"))
fn moonbit_test_driver_internal_string_read_char(handle : MoonbitTestDriverInternalStringReadHandle) -> Int = "__moonbit_fs_unstable" "string_read_char"

#cfg(any(target="wasm", target="wasm-gc"))
fn moonbit_test_driver_internal_finish_read_string(handle : MoonbitTestDriverInternalStringReadHandle) = "__moonbit_fs_unstable" "finish_read_string"

#cfg(any(target="wasm", target="wasm-gc"))
fn fixedarray_to_bytes(arr : FixedArray[Byte]) -> Bytes = "%identity"

#cfg(any(target="wasm", target="wasm-gc"))
fn moonbit_test_driver_internal_string_from_extern(e : MoonbitTestDriverInternalExternString) -> String {
  let buf = FixedArray::make(512, Byte::default())
  let mut len = 0
  let handle = moonbit_test_driver_internal_begin_read_string(e)
  while true {
    let ch = moonbit_test_driver_internal_string_read_char(handle)
    if ch == -1 {
      break
    } else {
      let written_bytes_len = buf.set_utf16le_char(len, moonbit_unsafe_char_from_int(ch))
      len += written_bytes_len
    }
  }
  moonbit_test_driver_internal_finish_read_string(handle)
  fixedarray_to_bytes(buf).to_unchecked_string(offset = 0, length = len).to_string()
}

// =========================================================
// =================== END WASM SPECIFIC ===================
// =========================================================

// =========================================================
// ================= BEGIN NATIVE SPECIFIC =================
// =========================================================
#cfg(target="native")
fn moonbit_test_driver_internal_native_parse_args() -> @moonbitlang/core/builtin.Array[(String, Int)] {
  fn moonbit_test_driver_internal_parse_int_(s : String) -> Int {
    let mut res = 0
    let len = s.length()
    for i = 0; i < len; i = i + 1 {
      res = res * 10 + (string_get(s, i) - '0')
    }
    res
  }

  fn moonbit_test_driver_internal_utf8_bytes_to_mbt_string(bytes : Bytes) -> String {
    let res = @moonbitlang/core/builtin.StringBuilder::new()
    let len = bytes.length()
    let mut i = 0
    while i < len {
      let mut c = bytes[i].to_int()
      if c < 0x80 {
        res.write_char(moonbit_unsafe_char_from_int(c))
        i += 1
      } else if c < 0xE0 {
        if i + 1 >= len {
          break
        }
        c = ((c & 0x1F) << 6) | (bytes[i + 1].to_int() & 0x3F)
        res.write_char(moonbit_unsafe_char_from_int(c))
        i += 2
      } else if c < 0xF0 {
        if i + 2 >= len {
          break
        }
        c = ((c & 0x0F) << 12) |
          ((bytes[i + 1].to_int() & 0x3F) << 6) |
          (bytes[i + 2].to_int() & 0x3F)
        res.write_char(moonbit_unsafe_char_from_int(c))
        i += 3
      } else {
        if i + 3 >= len {
          break
        }
        c = ((c & 0x07) << 18) |
          ((bytes[i + 1].to_int() & 0x3F) << 12) |
          ((bytes[i + 2].to_int() & 0x3F) << 6) |
          (bytes[i + 3].to_int() & 0x3F)
        c -= 0x10000
        res.write_char(moonbit_unsafe_char_from_int((c >> 10) + 0xD800))
        res.write_char(moonbit_unsafe_char_from_int((c & 0x3FF) + 0xDC00))
        i += 4
      }
    }
    res.to_string()
  }

  fn moonbit_test_driver_internal_get_cli_args_internal() -> @moonbitlang/core/builtin.Array[String] {
    let tmp = moonbit_test_driver_internal_get_cli_args_ffi()
    let res = @moonbitlang/core/builtin.Array::new(capacity=tmp.length())
    for i = 0; i < tmp.length(); i = i + 1 {
      res.push(moonbit_test_driver_internal_utf8_bytes_to_mbt_string(tmp[i]))
    }
    res
  }

  fn moonbit_test_driver_internal_split_mbt_string(s : String, sep : Char) -> @moonbitlang/core/builtin.Array[String] {
    let res = []
    let mut i = 0
    let mut start = 0
    while i < s.length() {
      if moonbit_unsafe_char_from_int(string_get(s, i)) == sep {
        res.push(s.unsafe_substring(start = start, end = i))
        start = i + 1
      }
      i += 1
    }
    if start < s.length() {
      res.push(s.unsafe_substring(start = start, end = s.length()))
    }
    res
  }

  let file_and_index = []

  let cli_args = moonbit_test_driver_internal_get_cli_args_internal()
  let test_args = moonbit_test_driver_internal_split_mbt_string(cli_args[1], '/')
  for arg in test_args {
    let file_and_range = moonbit_test_driver_internal_split_mbt_string(arg, ':')
    let file = file_and_range[0]
    let range = file_and_range[1]
    let start_and_end = moonbit_test_driver_internal_split_mbt_string(range, '-')
    let start = moonbit_test_driver_internal_parse_int_(start_and_end[0])
    let end = moonbit_test_driver_internal_parse_int_(start_and_end[1])
    for i = start; i < end; i = i + 1 {
      file_and_index.push((file, i))
    }
  }

  file_and_index
}

#cfg(target="native")
fn moonbit_test_driver_internal_get_cli_args_ffi() -> FixedArray[Bytes] = "$moonbit.get_cli_args"

// =========================================================
// ================== END NATIVE SPECIFIC ==================
// =========================================================

// =========================================================
// -================== BEGIN JS SPECIFIC ===================
// =========================================================

///|
#cfg(target="js")
#external
type MoonBitTestDriverInternalTestEntry

///|
#cfg(target="js")
extern "js" fn MoonBitTestDriverInternalTestEntry::filename(self : Self) -> String =
  #| (entry) => entry[0]

///|
#cfg(target="js")
extern "js" fn MoonBitTestDriverInternalTestEntry::index(self : Self) -> Int =
  #| (entry) => entry[1]

///|
pub(all) suberror MoonBitTestDriverInternalJsError String

///|
#cfg(target="js")
fn moonbit_test_driver_internal_catch_error(
  f : () -> Unit raise,
  on_ok~ : () -> Unit,
  on_err~ : (Error) -> Unit,
) -> Unit {
  moonbit_test_driver_internal_js_catch(
    on_err=(err_str) => on_err(MoonBitTestDriverInternalJsError(err_str)),
    () => {
      try f() catch {
        err => on_err(err)
      } noraise {
        _ => on_ok()
      }
    }
  )
}

///|
#cfg(target="js")
extern "js" fn moonbit_test_driver_internal_js_catch(
  f : () -> Unit,
  on_err~ : (String) -> Unit,
) =
  #| (f, on_err) => {
  #|   try {
  #|     f()
  #|   } catch (err) {
  #|     const msg = err.stack.toString()
  #|     on_err(msg)
  #|   }
  #| }

///|
#cfg(not(target="js"))
fn moonbit_test_driver_internal_catch_error(
  f : () -> Unit raise,
  on_ok~ : () -> Unit,
  on_err~ : (Error) -> Unit,
) -> Unit {
  try f() catch {
    err => on_err(err)
  } noraise {
    _ => on_ok()
  }
}

// =========================================================
// -=================== END JS SPECIFIC ====================
// =========================================================
