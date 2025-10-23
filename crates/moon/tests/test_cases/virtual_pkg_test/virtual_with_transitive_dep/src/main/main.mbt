///|
fn main {
  println(@middle.f())
  println(@virtual.f(44))
}

test "virtual package works" {
  let result = @virtual.f(10)
  if result != 11 {
    abort("Expected 11, got \{result}")
  }
}
