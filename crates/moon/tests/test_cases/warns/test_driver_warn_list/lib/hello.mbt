///|
priv struct Test(Int)

///|
priv struct Bench(Int)

///|
/// Returns a stable value for the warning-list test.
pub fn hello() -> Int {
  1
}

///|
test (t : @test.Test) {
  ignore(t)
}

///|
test "bench name collision" (b : @bench.T) {
  ignore(b)
}

///|
test {
  let Test(test_value) = Test(0)
  let Bench(bench_value) = Bench(0)
  ignore(test_value)
  ignore(bench_value)
  inspect(hello(), content="1")
}
