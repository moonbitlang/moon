///|
pub fn identity(x : Int64) -> Int64 where {
  proof_ensure: result => result == x,
} {
  x
}
