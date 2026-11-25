extern "C" fn uses_clang_with_static_msvc_crt() -> Int = "uses_clang_with_static_msvc_crt"

fn main {
  println(uses_clang_with_static_msvc_crt())
}
