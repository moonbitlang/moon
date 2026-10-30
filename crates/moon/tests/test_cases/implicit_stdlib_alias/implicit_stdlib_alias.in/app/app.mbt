///|
pub fn run(fail : Bool) -> String raise {
  if fail {
    raise Failure(@builtin.hello())
  }
  @builtin.hello() + " " + @prelude.hello()
}

///|
test {
  assert_eq(run(false), "util local builtin")
  try run(true) catch {
    Failure(msg) => assert_eq(msg, "util")
    _ => fail("unexpected error")
  } noraise {
    _ => fail("expected failure")
  }
}
