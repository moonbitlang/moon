
fn moonbit_test_driver_internal_get_file_name(file_name : MoonbitTestDriverInternalExternString) -> String {
  moonbit_test_driver_internal_string_from_extern(file_name)
}

#external
type MoonbitTestDriverInternalStringReadHandle

#external
type MoonbitTestDriverInternalExternString

fn moonbit_test_driver_internal_begin_read_string(s : MoonbitTestDriverInternalExternString) -> MoonbitTestDriverInternalStringReadHandle = "__moonbit_fs_unstable" "begin_read_string"

fn moonbit_test_driver_internal_string_read_char(handle : MoonbitTestDriverInternalStringReadHandle) -> Int = "__moonbit_fs_unstable" "string_read_char"

fn moonbit_test_driver_internal_finish_read_string(handle : MoonbitTestDriverInternalStringReadHandle) = "__moonbit_fs_unstable" "finish_read_string"

fn fixedarray_to_bytes(arr : FixedArray[Byte]) -> Bytes = "%identity"

fn moonbit_unsafe_char_from_int(x : Int) -> Char = "%identity"

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
