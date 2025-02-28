pub fn say_hello_3() -> Unit {
  say_hello_ffi_3()
}

extern "C" fn say_hello_ffi_3() = "say_hello_3"
