test "hello" {
  if @lib.hello() != "Hello, world!" {
    fail("@lib.hello() != \"Hello, world!\"")
  }
}

test "snapshot in blackbox test" (it : @test.T) {
  it.write(@lib.hello())
  it.snapshot!(filename="003.txt")
}
