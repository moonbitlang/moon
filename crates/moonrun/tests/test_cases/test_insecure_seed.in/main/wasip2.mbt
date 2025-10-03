///|
#cfg(target="wasm")
pub fn insecure_seed() -> (UInt64, UInt64) {
  let return_area = FixedArray::make(16, b'\x00')
  wasmImportInsecureSeed(byte_array2ptr(return_area))
  free(return_area)
  let view = return_area.unsafe_reinterpret_as_bytes()
  return (view[0:].to_uint64_le(), view[8:].to_uint64_le())
}

///|
#cfg(target="wasm-gc")
pub fn insecure_seed() -> (UInt64, UInt64) {
  wasmImportInsecureSeed(0)
  return (load64_u(0), load64_u(8))
}

///|
fn wasmImportInsecureSeed(p0 : Int) = "wasi:random/insecure-seed@0.2.0" "insecure-seed"
