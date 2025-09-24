// bench w/o BenchError raised
///|
test "bench" (it : @bench.T) {
  ignore(it)
}

// non-bench with BenchError raised
///|
test "non-bench" (it : @bench.T) {
  it.bench(fn() { ignore(42) })
}

// bench with BenchError raised
///|
test "bench" (it : @bench.T) {
  it.bench(fn() {  })
}

// fib bench
///|
test "bench: naive fib" (it : @bench.T) {
  let n = 20
  fn f(n : Int) {
    if n < 2 {
      return n
    }
    f(n - 1) + f(n - 2)
  }

  it.bench(fn() { f(n) |> ignore })
}

// non-bench w/o BenchError raised
///|
test {
  if hello() != "Hello, world!" {
    fail("not ok")
  }
}
