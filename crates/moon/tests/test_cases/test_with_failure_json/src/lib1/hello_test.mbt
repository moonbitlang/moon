test "hello" {
  if @lib1.hello() != "Hello, world!" {
    fail!("@lib.hello() != \"Hello, world!\"")
  }
}
