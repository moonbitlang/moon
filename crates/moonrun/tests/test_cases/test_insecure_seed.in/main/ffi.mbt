///|
#cfg(target="wasm")
#owned(buf)
pub extern "wasm" fn byte_array2ptr(buf : FixedArray[Byte]) -> Int =
  #|(func (param i32) (result i32) local.get 0 i32.const 8 i32.add)

///|
#cfg(target="wasm")
#borrow(buf)
pub extern "wasm" fn free(buf : FixedArray[Byte]) =
  #|(func (param i32) local.get 0 call $moonbit.decref)

///|
#cfg(target="wasm-gc")
extern "wasm" fn load8_u(offset : Int) -> Byte =
  #|(func (param i32) (result i32) local.get 0 i32.load8_u)

///|
#cfg(target="wasm-gc")
extern "wasm" fn load64_u(offset : Int) -> UInt64 =
  #|(func (param i32) (result i64) local.get 0 i64.load)
