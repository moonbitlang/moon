test "hello" {
  if @lib.hello() != "Hello, world!" {
    panic()
  }
}
