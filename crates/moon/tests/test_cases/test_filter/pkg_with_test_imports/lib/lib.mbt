test "hello_lib" {
  println(@lib1.hello_from_lib1())
  println(@lib2.hello_from_lib2())
}

pub fn hello() -> String {
  "hello from lib"
}
