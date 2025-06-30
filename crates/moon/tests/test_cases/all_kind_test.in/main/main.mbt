

fn main {
  println("hello main")
  T::{} |> ignore
}

/// ```mbt
///   println("doc test in main pkg")
/// ```
struct T {}

test {
  println("internal test in main pkg")
  inspect("update in internal test")
}