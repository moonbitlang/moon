test "hello" {
  if @lib.hello() != "Hello, world!" {
    fail("hello() != \"Hello, world!\"")
  }
}
