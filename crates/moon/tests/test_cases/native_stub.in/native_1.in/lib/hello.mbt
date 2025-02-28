pub fn say_hello_1() -> Unit {
  say_hello_ffi_1()
}

extern "C" fn say_hello_ffi_1() = "say_hello_1"
