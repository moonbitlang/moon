async test "a1" {
  let x = @async.suspend(k => k(42))
  assert_eq(x, 42)
}

async test "a2" {
  let x = @async.suspend(k => k(42))
  assert_eq(x, 0)
}
