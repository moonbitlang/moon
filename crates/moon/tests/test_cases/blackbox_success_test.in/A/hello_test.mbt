pub fn hello_bbtest() -> Unit {
  println("output from A/hello_bbtest.mbt!")
}

test {
  // current pkg
  @A.hello()
  // bbtest_import pkg
  @C.hello()
  // import pkg
  @D.hello()
}

test {
  let a = @A.new()
  a.method()
}
