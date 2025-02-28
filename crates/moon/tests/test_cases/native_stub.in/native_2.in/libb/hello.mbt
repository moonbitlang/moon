pub fn say_hello_2() -> Unit {
  say_hello_ffi_2()
}

extern "C" fn say_hello_ffi_2() = "say_hello_2"