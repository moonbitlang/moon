test {
  if add1(1) != 2 {
    fail("add1(1) should be 2")
  }
}

test {
  if @lib2.hello() != "hello" {
    fail("@lib2.hello() should be `hello`")
  }
}
