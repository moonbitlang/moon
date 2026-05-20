///|
fn fixedarray_as_bytes(arr : FixedArray[Byte]) -> Bytes = "%identity"

///|
#borrow(src)
#intrinsic("%utf16.len_from_utf8")
fn utf16_len_from_utf8(src : Bytes, src_offset : Int, src_length : Int) -> Int {
  let end = src_offset + src_length
  for index = src_offset, len = 0; index < end; {
    let byte = src.unsafe_get(index).to_int()
    if byte < 0x80 {
      continue index + 1, len + 1
    } else if byte < 0xC0 {
      continue index + 1, len
    } else if byte < 0xF0 {
      continue index + 1, len + 1
    } else {
      continue index + 1, len + 2
    }
  } nobreak {
    len
  }
}

///|
fn fill_utf8(size : Int) -> FixedArray[Byte] {
  let data = FixedArray::make(size, b'\x00')
  for index = 0; index < size; index = index + 2 {
    data.unsafe_set(index, b'\xC3')
    data.unsafe_set(index + 1, b'\xA9')
  }
  data
}

///|
fn run_work(input : Bytes, size : Int, iterations : Int) -> Int {
  for index = 0, checksum = 0; index < iterations; {
    let len = utf16_len_from_utf8(input, 0, size)
    let byte = input.unsafe_get(index * 131 % size).to_int()
    continue index + 1, checksum + len + byte
  } nobreak {
    checksum
  }
}

///|
fn main {
  let size = 16 * 1024 * 1024
  let iterations = 16
  let input = fixedarray_as_bytes(fill_utf8(size))
  ignore(run_work(input, size, 2))
  let start = @bench.monotonic_clock_start()
  let checksum = run_work(input, size, iterations)
  let elapsed_us = @bench.monotonic_clock_end(start)
  println(elapsed_us)
  println(checksum)
}
