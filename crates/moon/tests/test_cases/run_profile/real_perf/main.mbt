///|
fn fib(n : Int32) -> Int32 {
  if n < 2 {
    n
  } else {
    fib(n - 1) + fib(n - 2)
  }
}

///|
fn main {
  // Keep observable MoonBit work running long enough for 100 Hz sampling.
  let start = @bench.monotonic_clock_start()
  let result = for sum = 0 {
    let next = sum + fib(38 + (sum & 1))
    if @bench.monotonic_clock_end(start) >= 1_000_000.0 {
      break next
    }
    continue next
  }
  println(result)
}
