pub fn probe() -> Int {
  @builtin.one()
}

test "probe" {
  ignore(probe())
}
