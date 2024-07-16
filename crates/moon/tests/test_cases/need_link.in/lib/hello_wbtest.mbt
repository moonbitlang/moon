test "hello" {
  if hello() != "Hello, world!" {
    return Err("hello() != \"Hello, world!\"")
  }
}
