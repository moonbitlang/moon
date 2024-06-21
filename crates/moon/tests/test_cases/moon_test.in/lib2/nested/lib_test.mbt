test {
  if add1(1) != 2 {
    return Err("add1(1) should be 2")
  }
}

test {
  if @lib2.hello() != "hello" {
    return Err("@lib2.hello() should be `hello`")
  }
}
