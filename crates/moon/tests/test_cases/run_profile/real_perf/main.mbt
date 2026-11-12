///|
fn fib(n : Int32) -> Int32 {
  if n < 2 {
    n
  } else {
    fib(n - 1) + fib(n - 2)
  }
}

///|
fn workload() -> Unit {
  // Keep observable MoonBit work running long enough for 100 Hz sampling.
  let deadline = @env.now() + 1_000
  let result = for sum = 0 {
    let next = sum + fib(38 + (sum & 1))
    if @env.now() >= deadline {
      break next
    }
    continue next
  }
  println(result)
}

///|
fn main {
  workload()
}
