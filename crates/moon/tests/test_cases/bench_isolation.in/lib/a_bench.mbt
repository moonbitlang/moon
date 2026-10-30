// The two benchmarks below must not observe each other: `moon bench` gives
// each one its own process, so the second always starts from a clean slate.

///|
let ran_before : Array[Int] = [0]

///|
test "leaves state behind" (it : @bench.T) {
  ran_before[0] = ran_before[0] + 1
  it.bench(fn() { () })
}

///|
test "must start from a fresh process" (it : @bench.T) {
  if ran_before[0] != 0 {
    fail("another benchmark ran in this process first")
  }
  it.bench(fn() { () })
}
