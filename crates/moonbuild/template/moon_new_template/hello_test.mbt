test "hello" {
  if @lib.hello() != "Hello, world!" {
    raise "@lib.hello() != \"Hello, world!\""
  }
}
