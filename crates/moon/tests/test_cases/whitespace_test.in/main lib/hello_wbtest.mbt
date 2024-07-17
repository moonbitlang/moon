test "hello" {
  if hello() != "Hello, world!" {
    raise "hello() != \"Hello, world!\""
  }
}
