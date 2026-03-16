#requires(sorted(xs))
#ensures(binary_search_ok(xs, key, result))
pub fn binary_search(xs : FixedArray[Int], key : Int) -> Int {
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
    invariant: 0 <= i,
    invariant: i <= j,
    invariant: j <= xs.length(),
  }
}
