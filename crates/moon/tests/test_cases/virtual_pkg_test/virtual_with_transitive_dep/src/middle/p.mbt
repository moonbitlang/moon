///|
pub fn f() -> Int {
  @virtual.f(42)
}

test "middle uses virtual" {
  let result = f()
  if result != 43 {
    abort("Expected 43, got \{result}")
  }
}
