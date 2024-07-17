pub fn hello_test() -> Unit {
  println("output from A/hello_test.mbt!")
}

test {
  // it's ok for _test.mbt to use test_import
  @B.hello()
}

test {
  // should failed, bbtest_import could no be used in _test.mbt
  @C.hello()
}