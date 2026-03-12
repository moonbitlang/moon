test "liba" {
  if @lib.hello() != "workspace" {
    fail("@lib.hello() != \"workspace\"")
  }
}
