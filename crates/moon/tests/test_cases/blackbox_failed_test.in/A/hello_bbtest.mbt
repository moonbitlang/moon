pub fn hello_bbtest() -> Unit {
  println("output from A/hello_bbtest.mbt!")
}

test {
  let a = 1
  let mut b = 1
  // should failed
  @A._private_hello()
}