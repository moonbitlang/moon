///|
async fn workload() -> Unit {
  // Alternate CPU work with sleeps so the profile includes running and
  // suspended tasks. Printing the checksum keeps the work observable.
  let result = for batch = 0, checksum = 1; batch < 20; {
    let next = for i = 0, value = checksum; i < 10_000_000; {
      continue i + 1, (value * 33) ^ (value >> 5) ^ i
    } nobreak {
      value
    }
    @async.sleep(50)
    continue batch + 1, next
  } nobreak {
    checksum
  }
  println(result)
}

///|
async fn main {
  workload()
}
