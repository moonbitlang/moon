pub fn binary_search(xs : FixedArray[Int], key : Int) -> Int where {
  proof_require: sorted(xs),
  proof_ensure: result => binary_search_ok(xs, key, result), 
} {
  for i = 0, j = xs.length(); i < j; {
    let h = i + (j - i) / 2
    if xs[h] < key {
      continue h + 1, j
    } else {
      continue i, h
    }
  } nobreak {
    if i < xs.length() && xs[i] == key {
      i
    } else {
      -1
    }
  } where {
    proof_invariant: search_window(xs, i, j),
  }
}
