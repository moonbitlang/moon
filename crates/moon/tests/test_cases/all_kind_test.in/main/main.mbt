

fn main {
  println("hello main")
}

/// ```mbt
///   println("doc test in main pkg")
/// ```

test {
  println("internal test in main pkg")
  inspect("update in internal test")
}