
/// ```mbt
///   let unused_in_lib_doc_test = 1
///   println("doc test 1")
///   inspect("update in doc test")
/// ```
pub fn hello() -> String {
  "Hello, world!"
}

/// ```mbt
///   println("doc test 2")
/// ```
///


test {
  println("internal test 1")
}

test {
  println("internal test 2")
}