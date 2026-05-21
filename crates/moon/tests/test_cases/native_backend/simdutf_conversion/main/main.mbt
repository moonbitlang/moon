///|
fn fixedarray_as_bytes(arr : FixedArray[Byte]) -> Bytes = "%identity"

///|
fn string_from_uint16_fixedarray(arr : FixedArray[UInt16]) -> String = "%string.unsafe_from_uint16_fixedarray"

///|
fn put_utf16(dst : FixedArray[UInt16], offset : Int, codepoint : Int) -> Int {
  if codepoint <= 0xFFFF {
    dst.unsafe_set(offset, Int::to_uint16(codepoint))
    1
  } else {
    let ch = codepoint - 0x10000
    dst.unsafe_set(offset, Int::to_uint16((ch >> 10) + 0xD800))
    dst.unsafe_set(offset + 1, Int::to_uint16((ch & 0x3FF) + 0xDC00))
    2
  }
}

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
#borrow(src, dst)
#intrinsic("%utf8.decode_into_utf16")
fn utf8_decode_into_utf16(
  src : Bytes,
  src_offset : Int,
  src_length : Int,
  dst : FixedArray[UInt16],
  dst_offset : Int,
) -> Int {
  let end = src_offset + src_length
  for index = src_offset, written = 0; index < end; {
    let b0 = src.unsafe_get(index).to_int()
    if b0 < 0x80 {
      dst.unsafe_set(dst_offset + written, Int::to_uint16(b0))
      continue index + 1, written + 1
    }
    if b0 >= 0xC2 && b0 < 0xE0 && index + 1 < end {
      let b1 = src.unsafe_get(index + 1).to_int()
      if (b1 & 0xC0) == 0x80 {
        let ch = ((b0 & 0x1F) << 6) | (b1 & 0x3F)
        dst.unsafe_set(dst_offset + written, Int::to_uint16(ch))
        continue index + 2, written + 1
      }
    }
    if b0 >= 0xE0 && b0 < 0xF0 && index + 2 < end {
      let b1 = src.unsafe_get(index + 1).to_int()
      let b2 = src.unsafe_get(index + 2).to_int()
      let valid_lead = (b0 == 0xE0 && b1 >= 0xA0 && b1 <= 0xBF) ||
        (b0 >= 0xE1 && b0 <= 0xEC && (b1 & 0xC0) == 0x80) ||
        (b0 == 0xED && b1 >= 0x80 && b1 <= 0x9F) ||
        (b0 >= 0xEE && b0 <= 0xEF && (b1 & 0xC0) == 0x80)
      if valid_lead && (b2 & 0xC0) == 0x80 {
        let ch = ((b0 & 0x0F) << 12) | ((b1 & 0x3F) << 6) | (b2 & 0x3F)
        dst.unsafe_set(dst_offset + written, Int::to_uint16(ch))
        continue index + 3, written + 1
      }
    }
    if b0 >= 0xF0 && b0 < 0xF5 && index + 3 < end {
      let b1 = src.unsafe_get(index + 1).to_int()
      let b2 = src.unsafe_get(index + 2).to_int()
      let b3 = src.unsafe_get(index + 3).to_int()
      let valid_lead = (b0 == 0xF0 && b1 >= 0x90 && b1 <= 0xBF) ||
        (b0 >= 0xF1 && b0 <= 0xF3 && (b1 & 0xC0) == 0x80) ||
        (b0 == 0xF4 && b1 >= 0x80 && b1 <= 0x8F)
      if valid_lead && (b2 & 0xC0) == 0x80 && (b3 & 0xC0) == 0x80 {
        let ch = ((b0 & 0x07) << 18) |
          ((b1 & 0x3F) << 12) |
          ((b2 & 0x3F) << 6) |
          (b3 & 0x3F)
        continue index + 4, written + put_utf16(dst, dst_offset + written, ch)
      }
    }
    break -(index - src_offset + 1)
  } nobreak {
    written
  }
}

///|
#borrow(src)
#intrinsic("%utf8.len_from_utf16")
fn utf8_len_from_utf16(src : String, src_offset : Int, src_length : Int) -> Int {
  let end = src_offset + src_length
  for index = src_offset, len = 0; index < end; {
    let unit = src.unsafe_get(index)
    let code = unit.to_int()
    if code < 0x80 {
      continue index + 1, len + 1
    } else if code < 0x800 {
      continue index + 1, len + 2
    } else if unit.is_leading_surrogate() || unit.is_trailing_surrogate() {
      continue index + 1, len + 2
    } else {
      continue index + 1, len + 3
    }
  } nobreak {
    len
  }
}

///|
#borrow(src, dst)
#intrinsic("%utf8.encode_from_utf16")
fn utf8_encode_from_utf16(
  src : String,
  src_offset : Int,
  src_length : Int,
  dst : FixedArray[Byte],
  dst_offset : Int,
) -> Int {
  let end = src_offset + src_length
  for index = src_offset, written = 0; index < end; {
    let unit = src.unsafe_get(index)
    let code = unit.to_int()
    if code < 0x80 {
      dst.unsafe_set(dst_offset + written, code.to_byte())
      continue index + 1, written + 1
    }
    if code < 0x800 {
      dst.unsafe_set(dst_offset + written, (0xC0 | (code >> 6)).to_byte())
      dst.unsafe_set(dst_offset + written + 1, (0x80 | (code & 0x3F)).to_byte())
      continue index + 1, written + 2
    }
    if unit.is_leading_surrogate() && index + 1 < end {
      let trail = src.unsafe_get(index + 1)
      if trail.is_trailing_surrogate() {
        let scalar = ((code - 0xD800) << 10) +
          (trail.to_int() - 0xDC00) +
          0x10000
        dst.unsafe_set(dst_offset + written, (0xF0 | (scalar >> 18)).to_byte())
        dst.unsafe_set(
          dst_offset + written + 1,
          (0x80 | ((scalar >> 12) & 0x3F)).to_byte(),
        )
        dst.unsafe_set(
          dst_offset + written + 2,
          (0x80 | ((scalar >> 6) & 0x3F)).to_byte(),
        )
        dst.unsafe_set(
          dst_offset + written + 3,
          (0x80 | (scalar & 0x3F)).to_byte(),
        )
        continue index + 2, written + 4
      }
    }
    if unit.is_leading_surrogate() || unit.is_trailing_surrogate() {
      break -(index - src_offset + 1)
    }
    dst.unsafe_set(dst_offset + written, (0xE0 | (code >> 12)).to_byte())
    dst.unsafe_set(
      dst_offset + written + 1,
      (0x80 | ((code >> 6) & 0x3F)).to_byte(),
    )
    dst.unsafe_set(dst_offset + written + 2, (0x80 | (code & 0x3F)).to_byte())
    continue index + 1, written + 3
  } nobreak {
    written
  }
}

///|
fn main {
  let valid : FixedArray[Byte] = [
    b'\x41', b'\xc2', b'\xa9', b'\xe4', b'\xb8', b'\xad', b'\xf0', b'\x9f', b'\x98',
    b'\x80', b'\x42',
  ]
  let valid_bytes = fixedarray_as_bytes(valid)
  println(utf16_len_from_utf8(valid_bytes, 0, 11))
  let decoded = FixedArray::make(6, (0 : UInt16))
  println(utf8_decode_into_utf16(valid_bytes, 0, 11, decoded, 0))
  let decoded_string = string_from_uint16_fixedarray(decoded)
  println(decoded_string)

  println(utf8_len_from_utf16(decoded_string, 0, 6))
  let encoded = FixedArray::make(11, b'\x00')
  println(utf8_encode_from_utf16(decoded_string, 0, 6, encoded, 0))
  println(encoded[0])
  println(encoded[10])

  let invalid : FixedArray[Byte] = [b'\x41', b'\x80', b'\x42']
  println(
    utf8_decode_into_utf16(fixedarray_as_bytes(invalid), 0, 3, decoded, 0),
  )
  println("ok")
}
