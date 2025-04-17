test {
  if add1(1) != 2 {
    raise Failure("add1(1) should be 2")
  }
}

test {
  if @lib2.hello() != "hello" {
    raise Failure("@lib2.hello() should be `hello`")
  }
}
