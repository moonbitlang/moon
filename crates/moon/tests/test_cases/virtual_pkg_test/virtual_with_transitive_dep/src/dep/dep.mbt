///|
pub fn add_one(x : Int) -> Int {
  x + 1
}

test "dep works" {
  let result = add_one(5)
  if result != 6 {
    abort("Expected 6, got \{result}")
  }
}
