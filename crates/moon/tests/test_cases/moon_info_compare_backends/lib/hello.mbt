///|
pub fn hello() -> String {
  "Hello, world!"
}

///|
#cfg(target="wasm-gc")
pub fn hello_wasm_gc() -> String {
  "Hello from wasm-gc!"
}

///|
#cfg(target="wasm")
pub fn hello_wasm() -> String {
  "Hello from wasm!"
}

///|
#cfg(target="js")
pub fn hello_js() -> String {
  "Hello from js!"
}

///|
#cfg(target="native")
pub fn hello_native() -> String {
  "Hello from native!"
}
