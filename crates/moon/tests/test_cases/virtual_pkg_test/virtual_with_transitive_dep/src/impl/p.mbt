///|
pub fn f(x : Int) -> Int {
  @dep.add_one(x)
}

test "impl uses dep" {
  let result = f(10)
  if result != 11 {
    abort("Expected 11, got \{result}")
  }
}
