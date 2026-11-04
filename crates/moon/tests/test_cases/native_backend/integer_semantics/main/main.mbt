///|
fn main {
  // Runtime inputs keep the MoonBit compiler from folding the expressions.
  let args = @env.args()
  let hi32 = try! @string.parse_int(args[1])
  let lo32 = try! @string.parse_int(args[2])
  let hi64 = try! @string.parse_int64(args[3])
  let lo64 = try! @string.parse_int64(args[4])
  println(hi32 + 1)
  println(lo32 - 1)
  println(hi32 * 2)
  // These comparisons expose optimizations that assume signed overflow cannot
  // occur, even when printing the overflowed value itself appears to work.
  println(hi32 + 1 > hi32)
  println(lo32 - 1 < lo32)
  println(hi32 * 2 / 2 == hi32)
  println(hi64 + 1L)
  println(lo64 - 1L)
  println(hi64 * 2L)
  println(hi64 + 1L > hi64)
  println(lo64 - 1L < lo64)
  println(hi64 * 2L / 2L == hi64)
}
