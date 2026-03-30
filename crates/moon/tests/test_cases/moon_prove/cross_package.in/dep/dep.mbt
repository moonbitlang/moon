pub fn sum(a : Int, b : Int) -> Int where {
  proof_ensure: result => result == a + b,
} {
  let _ = a
  let _ = b
  0
}
