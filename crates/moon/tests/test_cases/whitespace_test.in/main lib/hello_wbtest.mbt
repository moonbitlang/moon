test "hello" {
  if hello() != "Hello, world!" {
    fail!("hello() != \"Hello, world!\"")
  }
}
