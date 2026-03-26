pub fn double(x : Int) -> Int where {
  proof_ensure: result => result == x * 2,
} { @dep.sum(x, x) }
