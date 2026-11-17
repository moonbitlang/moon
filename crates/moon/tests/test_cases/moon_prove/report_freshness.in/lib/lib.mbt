///|
pub fn inc(x : Int64) -> Int64 where {
  proof_require: small(x),
  proof_ensure: result => result > x,
} {
  x + 1L
}
