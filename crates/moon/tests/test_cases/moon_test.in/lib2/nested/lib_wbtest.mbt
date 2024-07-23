test {
  if add1(1) != 2 {
    raise "add1(1) should be 2"
  }
}

test {
  if @lib2.hello() != "hello" {
    raise "@lib2.hello() should be `hello`"
  }
}
