
/// ```mbt test
///   let unused_in_lib_doc_test = 1
///   println("doc test 1")
///   inspect("update in doc test")
/// ```
pub fn hello() -> String {
  S::{} |> ignore
 "Hello, world!"
}

/// ```mbt test
///   println("doc test 2")
/// ```
///
priv struct S {}

test {
  println("internal test 1")
}

test {
  println("internal test 2")
}