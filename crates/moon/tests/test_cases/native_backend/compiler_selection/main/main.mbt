extern "C" fn is_clang_cl() -> Int = "compiler_selection_is_clang_cl"

fn main {
  println(is_clang_cl())
}
