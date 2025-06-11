fn main {
  let args = get_args()
  println(args)
}

fn env_get_var(s : ExternString) -> ExternString = "__moonbit_fs_unstable" "env_get_var"

fn args_get() -> JSArray = "__moonbit_fs_unstable" "args_get"

pub fn get_env_var(name : String) -> String? {
  let res = env_get_var(string_to_extern(name))
  let mbt_string = string_from_extern(res)
  if mbt_string == "" {
    None
  } else {
    Some(mbt_string)
  }
}

pub fn get_args() -> Array[String] {
  let arr = args_get()
  let len = array_len(arr)
  let res = []
  for i = 0; i < len; i = i + 1 {
    let val = arr[i]
    if not(jsvalue_is_string(val)) {
      abort("Expected all strings in array")
    }
    res.push(string_from_extern(jsvalue_get_string(val)))
  }
  res
}

#external
type JSValue

pub fn jsvalue_is_string(v : JSValue) -> Bool = "__moonbit_fs_unstable" "jsvalue_is_string"

pub fn jsvalue_get_string(v : JSValue) -> ExternString = "%identity"

#external 
type JSArray

pub fn array_len(arr : JSArray) -> Int = "__moonbit_fs_unstable" "array_len"

pub fn array_get(arr : JSArray, idx : Int) -> JSValue = "__moonbit_fs_unstable" "array_get"

pub fn JSArray::op_get(self : JSArray, idx : Int) -> JSValue {
  return array_get(self, idx)
}

#external
type StringCreateHandle

#external
type StringReadHandle

#external
type ExternString

fn begin_create_string() -> StringCreateHandle = "__moonbit_fs_unstable" "begin_create_string"

fn string_append_char(handle : StringCreateHandle, ch : Char) = "__moonbit_fs_unstable" "string_append_char"

fn finish_create_string(handle : StringCreateHandle) -> ExternString = "__moonbit_fs_unstable" "finish_create_string"

pub fn string_to_extern(s : String) -> ExternString {
  let handle = begin_create_string()
  s.iter().each(fn(ch) { string_append_char(handle, ch) })
  finish_create_string(handle)
}

fn begin_read_string(s : ExternString) -> StringReadHandle = "__moonbit_fs_unstable" "begin_read_string"

/// Read one char from the string, returns -1 if the end of the string is reached.
/// The number returned is the unicode codepoint of the character.
fn string_read_char(handle : StringReadHandle) -> Int = "__moonbit_fs_unstable" "string_read_char"

fn finish_read_string(handle : StringReadHandle) = "__moonbit_fs_unstable" "finish_read_string"

pub fn string_from_extern(e : ExternString) -> String {
  let buf = @buffer.new()
  let handle = begin_read_string(e)
  while true {
    let ch = string_read_char(handle)
    if ch == -1 {
      break
    } else {
      buf.write_char(Char::from_int(ch))
    }
  }
  finish_read_string(handle)
  buf.to_string()
}
