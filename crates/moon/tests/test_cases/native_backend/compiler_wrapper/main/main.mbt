extern "C" fn answer() -> Int = "wrapper_test_answer"

fn main {
  println(answer())
}

test "C stub" {
  assert_eq(answer(), 42)
}
