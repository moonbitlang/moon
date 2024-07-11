pub fn hello_bbtest() -> Unit {
  println("output from A/hello_bbtest.mbt!")
}

test {
  // should failed
  @A._private_hello()
}