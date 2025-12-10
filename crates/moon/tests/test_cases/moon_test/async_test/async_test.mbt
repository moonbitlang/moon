async test "a1" {
  let x = 42
  @async.pause()
  assert_eq(x, 42)
}

async test "a2" {
  let x = 42
  @async.pause()
  assert_eq(x, 0)
}
