extern "C" fn c_answer() -> Int = "c_answer"
extern "C" fn cpp_answer() -> Int = "cpp_answer"

extern "C" fn other_answer() -> Int = "other_answer"

fn main {
  println(c_answer() + cpp_answer() + other_answer())
}
